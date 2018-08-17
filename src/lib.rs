#![feature(arbitrary_self_types)]
#![feature(futures_api)]
#![feature(pin)]

extern crate bytes;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate futures;
extern crate http;
extern crate httparse;

mod buffer;

use std::io;
use std::mem::PinMut;
use std::marker::Unpin;

use bytes::BytesMut;

use futures::future::Future;
use futures::io::AsyncRead;
use futures::task::{Context, Poll};

use http::Version;

use httparse::{Status, EMPTY_HEADER};

const BUFFER_SIZE: usize = 1024;

fn http_version(version: u8) -> Version {
    match version {
        0 => Version::HTTP_10,
        1 => Version::HTTP_11,
        _ => unreachable!(),
    }
}

/// A future that will resolve to a parsed http request header.
#[derive(Debug)]
pub struct ReadRequest<'a, A: 'a + AsyncRead + Unpin> {
    read: &'a mut A,
    buffer: &'a mut BytesMut,
    request: Option<buffer::Request>,
}

impl<'a, A: 'a + AsyncRead + Unpin> ReadRequest<'a, A> {
    /// Create a new HTTP request header parser - given both the reader to read from and a buffer
    /// to store excess bytes in
    pub fn new(reader: &'a mut A, buffer: &'a mut BytesMut) -> Self {
        ReadRequest {
            read: reader,
            buffer,
            request: Some(buffer::Request::new()),
        }
    }

    fn attempt_parse(self: PinMut<Self>, cx: &mut Context) -> Poll<Result<BytesMut, Error>> {
        let &mut ReadRequest {
            ref mut read,
            ref mut buffer,
            ref mut request,
        } = PinMut::get_mut(self);
        let request = request.as_mut().expect("calling poll on completed future");

        // do the actual read
        let mut tmp = [0; BUFFER_SIZE];
        match read.poll_read(cx, &mut tmp)? {
            Poll::Ready(count) => {
                buffer.extend_from_slice(&tmp[..count]);
            }
            Poll::Pending => {
                return Poll::Pending;
            }
        };

        // do the http parsing to see if its ready
        let status = {
            let mut headers = [EMPTY_HEADER; buffer::MAX_HEADERS];
            let mut parsing_request = unsafe { request.as_ref(&buffer, &mut headers[..]) };
            let status = parsing_request.parse(&buffer)?;
            *request = buffer::Request::from_ref(&buffer, &parsing_request);
            status
        };

        match status {
            Status::Complete(length) => Poll::Ready(Ok(buffer.split_to(length))),
            Status::Partial => Poll::Pending,
        }
    }

    fn attempt_type<'headers, 'buf: 'headers>(
        request: httparse::Request<'headers, 'buf>,
    ) -> Result<http::request::Parts, Error> {
        let mut partial_request = http::Request::builder();
        partial_request
            .method(request.method.unwrap())
            .uri(request.path.unwrap())
            .version(http_version(request.version.unwrap()));

        request
            .headers
            .iter()
            .take_while(|&header| header != &EMPTY_HEADER)
            .for_each(|&httparse::Header { name, value }| {
                partial_request.header(name, value);
            });

        let (output_request, ()) = partial_request.body(())?.into_parts();

        Ok(output_request)
    }
}

impl<'a, A: 'a + AsyncRead + Unpin> Future for ReadRequest<'a, A> {
    type Output = Result<http::request::Parts, Error>;

    fn poll(mut self: PinMut<Self>, cx: &mut Context) -> Poll<Self::Output> {
        let attempt = Self::attempt_parse(self.reborrow(), cx)?;
        let output_buffer = match attempt {
            Poll::Ready(buffer) => buffer,
            Poll::Pending => return Poll::Pending,
        };

        let request = PinMut::get_mut(self).request.take().unwrap();

        let mut headers = [EMPTY_HEADER; buffer::MAX_HEADERS];
        let raw_request = unsafe { request.as_ref(&output_buffer, &mut headers[..]) };
        let output_request = Self::attempt_type(raw_request)?;

        Poll::Ready(Ok(output_request))
    }
}

/// An error raised while attempting to parse a HTTP request from a readable object.
#[derive(Debug, Fail)]
pub enum Error {
    /// An error in underlying IO operations.
    #[fail(display = "{}", _0)]
    Io(#[cause] io::Error),
    /// An error in parsing the HTTP request sent over the network
    #[fail(display = "{}", _0)]
    Parse(#[cause] httparse::Error),
    /// An error in interpreting a parsed HTTP request as a strongly typed HTTP header set
    #[fail(display = "{}", _0)]
    Logic(#[cause] http::Error),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<httparse::Error> for Error {
    fn from(e: httparse::Error) -> Self {
        Error::Parse(e)
    }
}

impl From<http::Error> for Error {
    fn from(e: http::Error) -> Self {
        Error::Logic(e)
    }
}

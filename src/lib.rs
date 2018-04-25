extern crate bytes;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate futures;
extern crate http;
extern crate httparse;

mod buffer;

use std::io;

use bytes::BytesMut;

use futures::{Async, Future};
use futures::io::AsyncRead;
use futures::task::Context;

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

#[derive(Debug)]
struct ReadRequestState<A: AsyncRead> {
    read: A,
    buffer: BytesMut,
    request: buffer::Request,
}

/// A future that will resolve to a read and parsed http request.
#[derive(Debug)]
pub struct ReadRequest<A: AsyncRead> {
    data: Option<ReadRequestState<A>>,
}

impl<A: AsyncRead> ReadRequest<A> {
    pub fn new(reader: A, buffer: Option<BytesMut>) -> Self {
        ReadRequest {
            data: Some(ReadRequestState {
                read: reader,
                buffer: buffer.unwrap_or(BytesMut::new()),
                request: buffer::Request::new(),
            }),
        }
    }

    fn attempt_parse(&mut self, cx: &mut Context) -> Result<Async<BytesMut>, Error> {
        let &mut ReadRequestState {
            ref mut read,
            ref mut buffer,
            ref mut request,
        } = self.data.as_mut().expect("Called poll on completed future");

        // do the actual read
        let mut tmp = [0; BUFFER_SIZE];
        match read.poll_read(cx, &mut tmp)? {
            Async::Ready(count) => {
                buffer.extend_from_slice(&tmp[..count]);
            }
            Async::Pending => {
                return Ok(Async::Pending);
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
            Status::Complete(length) => Ok(Async::Ready(buffer.split_off(length))),
            Status::Partial => Ok(Async::Pending),
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

impl<A: AsyncRead> Future for ReadRequest<A> {
    type Item = (A, BytesMut, http::request::Parts);
    type Error = Error;

    fn poll(&mut self, cx: &mut Context) -> Result<Async<Self::Item>, Self::Error> {
        let attempt = self.attempt_parse(cx)?;
        println!("segcheck {:?}", attempt);
        let output_buffer = match attempt {
            Async::Ready(buffer) => buffer,
            Async::Pending => return Ok(Async::Pending),
        };
        println!("segcheck");

        let ReadRequestState {
            read,
            buffer,
            request,
        } = self.data.take().unwrap();

        let mut headers = [EMPTY_HEADER; buffer::MAX_HEADERS];
        let raw_request = unsafe { request.as_ref(&buffer, &mut headers[..]) };
        let output_request = Self::attempt_type(raw_request)?;

        Ok(Async::Ready((read, output_buffer, output_request)))
    }
}

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "{}", _0)]
    IoError(#[cause] io::Error),
    #[fail(display = "{}", _0)]
    HttpParseError(#[cause] httparse::Error),
    #[fail(display = "{}", _0)]
    HttpLogicError(#[cause] http::Error),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::IoError(e)
    }
}

impl From<httparse::Error> for Error {
    fn from(e: httparse::Error) -> Self {
        Error::HttpParseError(e)
    }
}

impl From<http::Error> for Error {
    fn from(e: http::Error) -> Self {
        Error::HttpLogicError(e)
    }
}

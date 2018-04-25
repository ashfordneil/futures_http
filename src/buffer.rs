//! Unsafe serializations of the httparse standard parse result types. Used to circumvent the
//! lifetimes of the httparse library in what is hopefully a reasonably safe and efficient manner,
//! so that buffers can be resized and possibly moved in memory without parsing needing to be
//! repeated.
//!
//! The serialization of these result types happens through repeated application of a simple
//! strategy. Any slice is converted to a raw offset from its buffer, and a length (in bytes). This
//! means that when converting these serialized types back to their slice counterparts, the buffer
//! used must either be the same, or have the same values in it.

use std::fmt::{self, Debug, Formatter};
use std::iter;
use std::mem;
use std::slice;
use std::str;

use httparse::{Header as RefHeader, Request as RefRequest, EMPTY_HEADER};

pub const MAX_HEADERS: usize = 100;

/// An unsafe serialization of a (partially) parsed request.
#[derive(Copy, Clone)]
pub struct Request {
    method: Option<(isize, usize)>,
    path: Option<(isize, usize)>,
    version: Option<u8>,
    headers: [Header; MAX_HEADERS],
    header_count: usize,
}

impl Debug for Request {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Request")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("version", &self.version)
            .field("headers", &&self.headers[..self.header_count])
            .finish()
    }
}

impl Request {
    /// Create a new, empty, serialized request.
    pub fn new() -> Self {
        Request {
            method: None,
            path: None,
            version: None,
            headers: unsafe { mem::uninitialized() },
            header_count: 0,
        }
    }

    /// Serializes a sliced request into an indexed request.
    pub fn from_ref<'headers, 'buf: 'headers>(
        buffer: &'buf [u8],
        req: &RefRequest<'headers, 'buf>,
    ) -> Self {
        let buffer = buffer.as_ptr() as isize;

        let relative_to_buffer =
            |slice: &str| (slice.as_ptr() as isize - buffer, slice.as_bytes().len());

        let method = req.method.map(&relative_to_buffer);
        let path = req.path.map(&relative_to_buffer);
        let version = req.version;
        let headers = {
            let mut headers: [Header; MAX_HEADERS] = unsafe { mem::uninitialized() };
            req.headers
                .iter()
                .zip(&mut headers[..].iter_mut())
                .for_each(|(header, target)| *target = Header::from_ref(buffer, header));
            headers
        };
        let header_count = req.headers.len();

        Request {
            method,
            path,
            version,
            headers,
            header_count,
        }
    }

    /// Convert an indexed request to a sliced request. This function may fail, as it requires
    /// there to be enough room in both the header buffer and the main buffer for all of this
    /// request's data. If any buffer is not large enough this function will panic.
    pub unsafe fn as_ref<'headers, 'buf: 'headers>(
        &self,
        buffer: &'buf [u8],
        headers_buffer: &'headers mut [RefHeader<'buf>],
    ) -> RefRequest<'headers, 'buf> {
        assert!(self.header_count < headers_buffer.len());

        let buffer_len = buffer.len();
        let buffer = buffer.as_ptr() as isize;

        let relative_to_buffer = |(start, length)| {
            assert!(start + length as isize <= buffer_len as isize);
            slice::from_raw_parts::<'buf, u8>((buffer + start) as *mut u8, length)
        };

        let method = self.method
            .map(&relative_to_buffer)
            .map(|buffer| str::from_utf8_unchecked(buffer));
        let path = self.path
            .map(&relative_to_buffer)
            .map(|buffer| str::from_utf8_unchecked(buffer));
        let version = self.version;
        let headers = {
            self.headers
                .iter()
                .take(self.header_count)
                .map(|header| header.as_ref(buffer, buffer_len))
                .chain(iter::repeat(EMPTY_HEADER))
                .zip(headers_buffer.iter_mut())
                .for_each(|(header, target)| *target = header);
            headers_buffer
        };

        RefRequest {
            method,
            path,
            version,
            headers,
        }
    }

}

#[derive(Copy, Clone, Debug)]
pub struct Header {
    name: (isize, usize),
    value: (isize, usize),
}

impl Header {
    /// Serializes a sliced header into an indexed header.
    pub fn from_ref<'headers, 'buf: 'headers>(buffer: isize, header: &RefHeader<'headers>) -> Self {
        let relative_to_buffer = |slice: &[u8]| (slice.as_ptr() as isize - buffer, slice.len());

        let name = relative_to_buffer(header.name.as_bytes());
        let value = relative_to_buffer(header.value);

        Header { name, value }
    }

    /// Convert an indexed header into a sliced header. This function may fail, as it requires
    /// there to be enough room in the buffer for all of this request's data. If the buffer is not
    /// large enough this function will panic.
    pub unsafe fn as_ref<'headers, 'buf: 'headers>(
        &self,
        buffer: isize,
        buffer_len: usize,
    ) -> RefHeader<'buf> {
        let relative_to_buffer = |(start, length)| {
            assert!(start + length as isize <= buffer_len as isize);
            slice::from_raw_parts::<'buf, u8>((buffer + start) as *mut u8, length)
        };

        let name = str::from_utf8_unchecked(relative_to_buffer(self.name));
        let value = relative_to_buffer(self.value);

        RefHeader { name, value }
    }
}

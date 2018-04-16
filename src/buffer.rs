use std::fmt::{self, Debug, Formatter};
use std::mem;
use std::slice;
use std::str;

use httparse::{Header as RefHeader, Request as RefRequest};

const MAX_HEADERS: usize = 100;

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
        write!(
            f,
            "Request {{ method: {:?}, path: {:?}, version: {:?}, headers: [",
            self.method, self.path, self.version
        )?;
        for i in 0..(self.header_count - 1) {
            write!(f, "{:?}, ", self.headers[i])?;
        }
        write!(f, "{:?}] }}", self.headers[self.header_count - 1])
    }
}

impl Request {
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
    pub unsafe fn as_ref<'headers, 'buf: 'headers>(
        &self,
        buffer: &'buf [u8],
        headers_buffer: &'headers mut [RefHeader<'buf>],
    ) -> Option<RefRequest<'headers, 'buf>> {
        if self.header_count > headers_buffer.len() {
            return None;
        }

        let buffer = buffer.as_ptr() as isize;

        let relative_to_buffer = |(start, length)| {
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
                .zip(headers_buffer.iter_mut())
                .for_each(|(header, target)| *target = header.as_ref(buffer));
            &mut headers_buffer[..self.header_count]
        };

        Some(RefRequest {
            method,
            path,
            version,
            headers,
        })
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Header {
    name: (isize, usize),
    value: (isize, usize),
}

impl Header {
    pub fn from_ref<'headers, 'buf: 'headers>(buffer: isize, header: &RefHeader<'headers>) -> Self {
        let relative_to_buffer = |slice: &[u8]| (slice.as_ptr() as isize - buffer, slice.len());

        let name = relative_to_buffer(header.name.as_bytes());
        let value = relative_to_buffer(header.value);

        Header { name, value }
    }

    pub unsafe fn as_ref<'headers, 'buf: 'headers>(&self, buffer: isize) -> RefHeader<'buf> {
        let relative_to_buffer = |(start, length)| {
            slice::from_raw_parts::<'buf, u8>((buffer + start) as *mut u8, length)
        };

        let name = str::from_utf8_unchecked(relative_to_buffer(self.name));
        let value = relative_to_buffer(self.value);

        RefHeader { name, value }
    }
}

#![feature(futures_api, async_await, await_macro)]
extern crate bytes;
extern crate failure;
extern crate futures;
extern crate future_http;
extern crate toykio;

use bytes::BytesMut;

use failure::Error;

use futures::stream::StreamExt;

use future_http::ReadRequest;

use toykio::{AsyncTcpStream, AsyncTcpListener};

async fn handle_conn(mut sock: AsyncTcpStream) {
    let mut buffer = BytesMut::new();
    let headers = await!(ReadRequest::new(&mut sock, &mut buffer));
    println!("{:?}", headers);
}

async fn run(listener: AsyncTcpListener) {
    let mut incoming = listener.incoming();
    loop {
        let (sock, new_incoming) = await!(incoming.into_future());
        incoming = new_incoming;
        toykio::spawn(handle_conn(sock.unwrap()));
    }
}

fn main() -> Result<(), Error> {
    let listener = AsyncTcpListener::bind(("0.0.0.0", 8000))?;
    toykio::run(run(listener));

    Ok(())
}

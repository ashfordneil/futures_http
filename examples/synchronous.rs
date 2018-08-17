#![feature(futures_api, async_await, await_macro)]
extern crate bytes;
extern crate failure;
extern crate futures;
extern crate future_http;

use std::net::{TcpStream, TcpListener};

use bytes::BytesMut;

use failure::Error;

use futures::executor;
use futures::io::AllowStdIo;

use future_http::ReadRequest;

async fn handle_conn(sock: TcpStream) {
    let mut wrapper = AllowStdIo::new(sock);
    let mut buffer = BytesMut::new();
    let headers = await!(ReadRequest::new(&mut wrapper, &mut buffer));
    println!("{:?}", headers);
}

async fn run(listener: TcpListener) {
    let incoming = listener.incoming();
    for sock in incoming {
        await!(handle_conn(sock.unwrap()));
    }
}

fn main() -> Result<(), Error> {
    let listener = TcpListener::bind(("0.0.0.0", 8000))?;
    executor::block_on(run(listener));

    Ok(())
}

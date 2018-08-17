#![feature(futures_api, async_await, await_macro, pin)]
extern crate bytes;
extern crate failure;
extern crate futures;
extern crate future_http;
#[macro_use]
extern crate pin_utils;

use std::net::{TcpStream, TcpListener};

use bytes::BytesMut;

use failure::Error;

use futures::io::AllowStdIo;

use future_http::ReadRequest;

mod executor {
    use std::sync::Arc;

    use futures::Future;
    use futures::future::FutureObj;
    use futures::task::{self, Poll, Context, Wake, Spawn, SpawnObjError};

    struct Waker;
    impl Wake for Waker {
        fn wake(_arc_self: &Arc<Waker>) {
            // does nothing
        }
    }

    struct Executor;
    impl Spawn for Executor {
        fn spawn_obj(&mut self, _future: FutureObj<'static, ()>) -> Result<(), SpawnObjError> {
            unimplemented!()
        }
    }

    pub fn block_serial_future<T>(future: impl Future<Output = T>) -> T {
        pin_mut!(future);

        let waker = {
            let general = Waker;
            let local = task::local_waker_from_nonlocal(Arc::new(general));
            local
        };
        let mut executor = Executor;
        let mut context = Context::new(&waker, &mut executor);
        loop {
            if let Poll::Ready(output) = future.reborrow().poll(&mut context) {
                break output;
            }
        }
    }
}

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
    let future = run(listener);
    executor::block_serial_future(future);
    Ok(())
}

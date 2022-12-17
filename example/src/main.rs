use std::marker::PhantomData;

use driver::DriverService;
use piton::{ClientTransport, ServerTransport};
use piton_bbq::*;

#[allow(unused_variables)]
pub mod driver {
    include!(concat!(env!("OUT_DIR"), "/foo.rs"));
}

#[derive(Default)]
pub struct Service<T> {
    phantom: PhantomData<T>,
}

impl<T> Service<T> {
    pub fn new() -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

impl<T: ServerTransport> DriverService<T> for Service<T> {
    fn xyz<'t>(
        &mut self,
        msg: &mut driver::Bar,
        resp: &mut piton::TypedBuf<T::BufW<'t>, driver::Test>,
    ) -> Result<piton::InsertToken<T::BufW<'t>, driver::Test>, T::Error> {
        println!("xyz: {:?}", msg);
        Ok(resp.insert(driver::Test {
            foo: 2,
            bar: 0xFF,
            boolean: false,
            array: [0; 20],
        }))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut server = Server::default();
    let client = server.client();
    let server = std::thread::spawn(move || {
        let server = driver::DriverServer {
            transport: server,
            service: Service::<Server<{ 4096 * 4 }>>::new(),
        };
        server.run()
    });
    let client = std::thread::spawn(move || {
        let mut client = driver::DriverClient { transport: client };
        loop {
            let mut msg = client.transport.alloc_typed().unwrap();
            msg.insert(driver::Bar::B(0xAB));
            let resp = client.xyz(msg).unwrap();
            println!("{:?}", resp.as_ref());
        }
    });
    server.join().unwrap().unwrap();
    client.join().unwrap();
    Ok(())
}

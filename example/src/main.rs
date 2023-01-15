use std::marker::PhantomData;

use bytecheck::CheckBytes;
use driver::DriverService;
use piton::{ServiceRx, ServiceTx};
use piton_bbq::*;

#[allow(unused_variables)]
pub mod driver {
    include!(concat!(env!("OUT_DIR"), "/foo.rs"));
}

#[derive(Default)]
pub struct Service<T, D> {
    phantom: PhantomData<T>,
    phantom_d: PhantomData<D>,
}

impl<T, D> Service<T, D> {
    pub fn new() -> Self {
        Self {
            phantom: PhantomData,
            phantom_d: PhantomData,
        }
    }
}

impl<T: ServiceRx, D: CheckBytes<()> + Default + Clone + std::fmt::Debug + 'static>
    DriverService<T, D> for Service<T, D>
{
    fn xyz<'t>(
        &mut self,
        msg: &mut driver::Bar<D>,
        resp: &mut piton::TypedBuf<T::BufW<'t>, driver::Test<D>>,
    ) -> Result<piton::InsertToken<T::BufW<'t>, driver::Test<D>>, piton::Error> {
        println!("xyz: {:?}", msg);
        let bar = match msg {
            driver::Bar::Test => Default::default(),
            driver::Bar::B(b) => b.clone(),
        };
        Ok(resp.insert(driver::Test {
            foo: 2,
            bar,
            boolean: false,
            array: [0; 20],
            ..Default::default()
        }))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut server = Server::default();
    let client = server.client();
    let server = std::thread::spawn(move || {
        let server = driver::DriverServer::new(server, Service::<Server<{ 4096 * 4 }>, u16>::new());
        server.run()
    });
    let client = std::thread::spawn(move || {
        let mut client = driver::DriverClient::<_, u16>::new(client);
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

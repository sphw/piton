use std::marker::PhantomData;

use core::ops::Deref;
use driver::DriverService;
use piton::ServiceRx;
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

impl<T: ServiceRx, D: piton::Yule + Default + Clone + std::fmt::Debug + 'static> DriverService<T, D>
    for Service<T, D>
{
    fn xyz<'id>(
        &mut self,
        msg: &driver::Bar<D>,
        resp: &mut driver::Test<D>,
    ) -> Result<(), piton::Error> {
        println!("serv got {:?}", msg);
        *resp = driver::Test {
            array: [0xFF; 20],
            ..Default::default()
        };
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut server = Server::default();
    let client = server.client();
    let server = std::thread::spawn(move || {
        let server =
            driver::DriverServer::new(server, Service::<Server<{ 4096 * 16 }, _, _>, u16>::new());
        server.run()
    });
    let client = std::thread::spawn(move || {
        let mut client = driver::DriverClient::<_, u16>::new(client);
        loop {
            let mut call_ref = client.xyz_ref().unwrap();
            *call_ref = driver::Bar::B(0xFF);
            let resp = call_ref.call().unwrap();
            println!("resp {:?}", resp.deref());
            drop(resp);
        }
    });
    server.join().unwrap().unwrap();
    client.join().unwrap();
    Ok(())
}

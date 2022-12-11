use std::marker::PhantomData;

use driver::DriverService;
use piton::{Buf as _, ClientTransport, Responder, ServerTransport, TypedBuf as _};

#[allow(unused_variables)]
pub mod driver {
    include!(concat!(env!("OUT_DIR"), "/foo.rs"));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = flume::unbounded();
    let server = Server { rx };
    let client = Client { tx };
    let server = std::thread::spawn(move || {
        let server = driver::DriverServer {
            transport: server,
            service: Service::<Server>::new(),
        };
        server.run()
    });
    let client = std::thread::spawn(move || {
        let mut client = driver::DriverClient { transport: client };
        loop {
            let mut msg = client.transport.alloc().unwrap();
            msg.insert(driver::Bar::B(0xAB));
            println!("send");
            client.send(msg).unwrap();
        }
    });
    server.join().unwrap().unwrap();
    client.join().unwrap();
    Ok(())
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
    fn send<'t>(
        &mut self,
        msg: &mut driver::Bar,
        resp: &mut T::TypedBuf<'t, driver::Test>,
    ) -> Result<piton::InsertToken<'t, T, driver::Test>, T::Error> {
        println!("send called: {:?}", msg);
        Ok(resp.insert(driver::Test {
            foo: 2,
            bar: 0xFF,
            boolean: false,
            array: [0; 20],
        }))
    }
}

struct Buf(Option<Box<[u8]>>);
struct TypedBuf<T> {
    inner: Buf,
    _phantom: PhantomData<T>,
}

struct InsertToken<T>(PhantomData<T>);

impl piton::Buf for Buf {
    type Error = ();
    fn insert<T>(&mut self, obj: T) -> Result<InsertToken<T>, Self::Error> {
        unsafe {
            let slice =
                std::slice::from_raw_parts(&obj as *const T as *const u8, std::mem::size_of::<T>());
            self.0 = Some(Box::from(slice));
            Ok(InsertToken(PhantomData))
        }
    }

    fn as_maybe_uninit<T>(&mut self) -> &mut std::mem::MaybeUninit<T> {
        todo!()
    }

    fn as_ref<T: bytecheck::CheckBytes<()>>(&self) -> Option<&T> {
        match &self.0 {
            Some(s) => unsafe { T::check_bytes(s.as_ptr() as *mut T, &mut ()).ok() },
            None => None,
        }
    }

    fn as_mut<T: bytecheck::CheckBytes<()>>(&mut self) -> Option<&mut T> {
        match &mut self.0 {
            Some(s) => unsafe {
                // TODO: check bounds maybe?
                let _ = T::check_bytes(s.as_mut_ptr() as *mut T, &mut ()).ok()?;
                Some(&mut *(s.as_mut_ptr() as *mut T))
            },
            None => None,
        }
    }

    type InsertToken<I> = InsertToken<I>;
}

impl<T: bytecheck::CheckBytes<()>> piton::TypedBuf<T> for TypedBuf<T> {
    type Buf = Buf;
    type InsertToken = InsertToken<T>;

    fn insert(&mut self, obj: T) -> InsertToken<T> {
        self.inner.insert(obj).unwrap()
    }

    fn as_maybe_uninit(&mut self) -> &mut std::mem::MaybeUninit<T> {
        todo!()
    }

    fn as_ref(&self) -> Option<&T> {
        todo!()
    }

    fn as_mut(&mut self) -> Option<&mut T> {
        todo!()
    }

    fn from_buf(inner: Self::Buf) -> Result<Self, <Self::Buf as piton::Buf>::Error>
    where
        Self: Sized,
    {
        Ok(TypedBuf {
            inner,
            _phantom: PhantomData,
        })
    }
}

struct Req {
    msg_type: u32,
    buf: Buf,
    tx: oneshot::Sender<Buf>,
}

struct Resp(oneshot::Sender<Buf>);

struct Server {
    rx: flume::Receiver<Req>,
}

impl ServerTransport for Server {
    type Responder<'a> = Resp;

    type Buf<'r> = Buf;

    type TypedBuf<'r, M> = TypedBuf<M> where M: bytecheck::CheckBytes<()> + 'r;

    type Error = ();

    fn recv(
        &mut self,
    ) -> Result<Option<piton::Recv<Self::Buf<'_>, Self::Responder<'_>>>, Self::Error> {
        let Req { buf, tx, msg_type } = self.rx.recv().unwrap();
        Ok(Some(piton::Recv {
            req: buf,
            resp: Buf(None),
            responder: Resp(tx),
            msg_type,
        }))
    }
}

impl Responder for Resp {
    type ServerTransport = Server;

    type Error = ();

    fn send<'r, 'm, M>(
        self,
        _req_type: u32,
        msg: <Self::ServerTransport as ServerTransport>::TypedBuf<'m, M>,
    ) -> Result<(), Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm,
    {
        self.0.send(msg.inner).unwrap();
        Ok(())
    }
}

struct Client {
    tx: flume::Sender<Req>,
}

impl ClientTransport for Client {
    type Buf<'r, T: bytecheck::CheckBytes<()>> = TypedBuf<T> where T: 'r;

    type Error = ClientError;

    fn call<'r, 'm, M, R>(
        &'r mut self,
        req_type: u32,
        msg: Self::Buf<'m, M>,
    ) -> Result<Self::Buf<'r, R>, Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm,
        R: bytecheck::CheckBytes<()> + 'r,
    {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Req {
            msg_type: req_type,
            buf: msg.inner,
            tx,
        })?;
        let buf = rx.recv()?;
        Ok(TypedBuf {
            inner: buf,
            _phantom: PhantomData,
        })
    }

    fn send<'r, 'm, M, R>(
        &'r mut self,
        req_type: u32,
        msg: Self::Buf<'m, M>,
    ) -> Result<(), Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm,
    {
        let (tx, _) = oneshot::channel();
        self.tx.send(Req {
            msg_type: req_type,
            buf: msg.inner,
            tx,
        })?;
        Ok(())
    }

    fn alloc<'r, T>(&mut self) -> Result<Self::Buf<'r, T>, Self::Error>
    where
        T: bytecheck::CheckBytes<()> + 'r,
    {
        Ok(TypedBuf {
            inner: Buf(None),
            _phantom: PhantomData,
        })
    }
}

#[derive(thiserror::Error, Debug)]
enum ClientError {
    #[error("oneshot recv: {0}")]
    OneshotRecv(#[from] oneshot::RecvError),
    #[error("flume send: {0}")]
    FlumeSend(#[from] flume::SendError<Req>),
}

extern crate alloc;

use alloc::sync::Arc;
use core::mem::align_of;
use core::{
    mem::{size_of, MaybeUninit},
    sync::atomic::{AtomicUsize, Ordering},
};

use bbqueue::{framed::FrameGrantR, BufStorage};
use piton::ServerTransport;

pub type Storage<const N: usize = { 4094 * 4 }> = Arc<BufStorage<N>>;

pub struct Tx<const N: usize> {
    signal: Arc<AtomicUsize>,
    prod: bbqueue::framed::FrameProducer<Storage<N>>,
}

pub struct Rx<const N: usize> {
    signal: Arc<AtomicUsize>,
    cons: bbqueue::framed::FrameConsumer<Storage<N>>,
}

pub struct Server<const N: usize> {
    queue: bbqueue::BBBuffer<Storage<N>>,
    rx: Rx<N>,
    tx: Vec<Tx<N>>,
}

impl<const N: usize> Default for Server<N> {
    fn default() -> Self {
        let queue = bbqueue::BBBuffer::new(Arc::new(BufStorage::new()));
        let cons = queue.frame_consumer().expect("consumer already created");
        Server {
            queue,
            rx: Rx {
                cons,
                signal: Arc::new(AtomicUsize::new(0)),
            },
            tx: vec![],
        }
    }
}

impl<const N: usize> Server<N> {
    pub fn client(&mut self) -> Client<N> {
        let reply = bbqueue::BBBuffer::new(Arc::new(BufStorage::new()));
        let id = self.tx.len();
        let signal = Arc::new(AtomicUsize::new(0));
        self.tx.push(Tx {
            prod: reply.frame_producer(),
            signal: signal.clone(),
        });
        Client {
            tx: Tx {
                prod: self.queue.frame_producer(),
                signal: self.rx.signal.clone(),
            },
            rx: Rx {
                cons: reply.frame_consumer().expect("consumer already created"),
                signal,
            },
            id,
        }
    }
}

impl<const N: usize> ServerTransport for Server<N> {
    type Responder<'a> = Responder<'a, N>;

    type BufR<'r> = BufR<N>;

    type BufW<'r> = BufW<N>;

    type Error = ();

    fn recv(
        &mut self,
    ) -> Result<Option<piton::Recv<Self::BufW<'_>, Self::BufR<'_>, Self::Responder<'_>>>, Self::Error>
    {
        let mut buf = self.rx.recv();
        let id = usize::from_be_bytes(buf[..{ size_of::<usize>() }].try_into().map_err(|_| ())?);
        let msg_type = u32::from_be_bytes(
                buf[{ size_of::<usize>() }..HEADER_LENGTH]
                .try_into()
                .map_err(|_| ())?,
        );
        let tx = &mut self.tx[id];
        let resp = BufW(tx.prod.grant(1024).unwrap());
        buf.auto_release(true);
        Ok(Some(piton::Recv {
            req: BufR(buf),
            resp,
            responder: Responder {
                signal: tx.signal.as_ref(),
            },
            msg_type,
        }))
    }
}

pub struct Responder<'a, const N: usize> {
    signal: &'a AtomicUsize,
}

impl<'a, const N: usize> piton::Responder for Responder<'a, N> {
    type ServerTransport = Server<N>;

    type Error = ();

    fn send<'r, 'm, M>(
        self,
        _req_type: u32,
        msg: piton::TypedBuf<<Self::ServerTransport as ServerTransport>::BufW<'m>, M>,
    ) -> Result<(), Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm,
    {
        msg.buf.commit::<M>();
        self.signal.fetch_add(1, Ordering::Release);
        Ok(())
    }
}

pub struct Client<const N: usize> {
    id: usize,
    tx: Tx<N>,
    rx: Rx<N>,
}

impl<const N: usize> piton::ClientTransport for Client<N> {
    type BufW<'r> = BufW<N>;

    type BufR<'r> = BufR<N>;

    type Error = ();

    fn call<'r, 'm, M, R>(
        &'r mut self,
        req_type: u32,
        mut msg: piton::TypedBuf<Self::BufW<'m>, M>,
    ) -> Result<piton::TypedBuf<Self::BufR<'r>, R>, Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm,
        R: bytecheck::CheckBytes<()> + 'r,
    {
        msg.buf.0[0..{ size_of::<usize>() }].copy_from_slice(&self.id.to_be_bytes());
        msg.buf.0[{ size_of::<usize>() }..HEADER_LENGTH].copy_from_slice(&req_type.to_be_bytes());
        msg.buf.commit::<M>();
        self.tx.signal.fetch_add(1, Ordering::Release);
        let mut resp = self.rx.recv();
        resp.auto_release(true);
        Ok(piton::TypedBuf::new(BufR(resp)).unwrap())
    }

    fn alloc<'r>(&mut self, capacity: usize) -> Result<Self::BufW<'r>, Self::Error> {
        self.tx.prod.grant(capacity).map_err(|_| ()).map(BufW)
    }
}

const HEADER_LENGTH: usize = 4 + size_of::<usize>();

pub struct BufW<const N: usize>(bbqueue::framed::FrameGrantW<Storage<N>>);

impl<const N: usize> BufW<N> {
    fn commit<T>(self) {
        self.0
            .commit(size_of::<T>() + align_of::<T>() + HEADER_LENGTH)
    }
}
pub struct BufR<const N: usize>(bbqueue::framed::FrameGrantR<Storage<N>>);

macro_rules! impl_buf {
    ($struct_n:ident) => {
        impl<const N: usize> piton::Buf for $struct_n<N> {
            fn can_insert<T>(&self) -> bool {
                (core::mem::size_of::<T>() + align_of::<T>() + HEADER_LENGTH) <= self.0.len()
            }

            fn as_ref<T: bytecheck::CheckBytes<()>>(&self) -> Option<&T> {
                unsafe {
                    T::check_bytes(self.0.as_ptr().add(HEADER_LENGTH) as *mut T, &mut ()).ok()
                }
            }

            fn as_mut<T: bytecheck::CheckBytes<()>>(&mut self) -> Option<&mut T> {
                unsafe {
                    // TODO: check bounds maybe?
                    let _ = T::check_bytes(self.0.as_mut_ptr() as *mut T, &mut ()).ok()?;
                    Some(&mut *(self.0.as_mut_ptr().add(HEADER_LENGTH) as *mut T))
                }
            }

            // Safety: Assumes there is enough room for the object plus alignment
            unsafe fn as_maybe_uninit<T>(&mut self) -> &mut MaybeUninit<T> {
                let ptr = self.0.as_ptr().add(HEADER_LENGTH);
                let addr = ptr as usize;
                let align_mask = align_of::<T>() - 1;
                let ptr = if addr & align_mask == 0 {
                    ptr as *mut MaybeUninit<T>
                } else {
                    ((addr | align_mask) + 1) as *mut MaybeUninit<T>
                };
                &mut *ptr
            }
        }
    };
}

fn spin_wait(signal: &AtomicUsize) {
    while signal.load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
    }
    signal.fetch_sub(1, Ordering::Release);
}

impl<const N: usize> Rx<N> {
    fn recv(&mut self) -> FrameGrantR<Storage<N>> {
        spin_wait(self.signal.as_ref());
        self.cons.read().unwrap()
    }
}

impl_buf! { BufW }
impl_buf! { BufR }

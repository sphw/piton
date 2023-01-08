extern crate alloc;

use alloc::sync::Arc;
use core::mem::align_of;
use core::{
    mem::{size_of, MaybeUninit},
    sync::atomic::{AtomicUsize, Ordering},
};

use bbqueue::{framed::FrameGrantR, BufStorage};
use piton::{Error, ServiceRx};

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

impl<const N: usize> ServiceRx for Server<N> {
    type Responder<'a> = Responder<'a, N>;

    type BufR<'r> = BufR<N>;

    type BufW<'r> = BufW<N>;

    fn recv(
        &mut self,
    ) -> Result<Option<piton::Recv<Self::BufW<'_>, Self::BufR<'_>, Self::Responder<'_>>>, Error>
    {
        let mut buf = self.rx.recv();
        let id = usize::from_be_bytes(
            buf[..{ size_of::<usize>() }]
                .try_into()
                .map_err(|_| Error::BufferUnderflow)?,
        );
        let msg_type = u32::from_be_bytes(
            buf[{ size_of::<usize>() }..HEADER_LENGTH]
                .try_into()
                .map_err(|_| Error::BufferUnderflow)?,
        );
        let tx = &mut self.tx[id];
        let resp = BufW(tx.prod.grant(1024).map_err(|_| Error::BufferUnderflow)?);
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

    fn send<'r, 'm, M>(
        self,
        _req_type: u32,
        msg: piton::TypedBuf<<Self::ServerTransport as ServiceRx>::BufW<'m>, M>,
    ) -> Result<(), Error>
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

impl<const N: usize> piton::ServiceTx for Client<N> {
    type BufW<'r> = BufW<N>;

    type BufR<'r> = BufR<N>;

    fn call<'r, 'm, M, R>(
        &'r mut self,
        req_type: u32,
        mut msg: piton::TypedBuf<Self::BufW<'m>, M>,
    ) -> Result<piton::TypedBuf<Self::BufR<'r>, R>, Error>
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
        piton::TypedBuf::new(BufR(resp)).ok_or(Error::InvalidMsg)
    }

    fn alloc<'r>(&mut self, capacity: usize) -> Result<Self::BufW<'r>, Error> {
        self.tx
            .prod
            .grant(capacity)
            .map_err(|_| Error::BufferOverflow)
            .map(BufW)
    }
}

struct BusTx<const N: usize> {
    tx: Tx<N>,
}

impl<const N: usize> piton::BusTx for BusTx<N> {
    type BufW<'r> = BufW<N>;

    fn send<'r, 'm, M>(
        &'r mut self,
        req_type: u32,
        mut msg: piton::TypedBuf<Self::BufW<'m>, M>,
    ) -> Result<(), Error>
    where
        M: bytecheck::CheckBytes<()> + 'm,
    {
        msg.buf.0[{ size_of::<usize>() }..HEADER_LENGTH].copy_from_slice(&req_type.to_be_bytes());
        msg.buf.commit::<M>();
        self.tx.signal.fetch_add(1, Ordering::Release);
        Ok(())
    }

    fn alloc<'r>(&mut self, capacity: usize) -> Result<Self::BufW<'r>, Error> {
        self.tx
            .prod
            .grant(capacity)
            .map_err(|_| Error::BufferOverflow)
            .map(BufW)
    }
}

struct BusRx<const N: usize> {
    rx: Rx<N>,
}

impl<const N: usize> piton::BusRx for BusRx<N> {
    type BufR<'r> = BufR<N>;

    fn recv(&mut self) -> Result<Option<piton::Msg<Self::BufR<'_>>>, Error> {
        let mut buf = self.rx.recv();
        let msg_type = u32::from_be_bytes(
            buf[{ size_of::<usize>() }..HEADER_LENGTH]
                .try_into()
                .map_err(|_| Error::BufferUnderflow)?,
        );
        buf.auto_release(true);
        Ok(Some(piton::Msg {
            req: BufR(buf),
            msg_type,
        }))
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
                // TODO: fix safety here
                unsafe {
                    let ptr = align_up(
                        self.0.as_ptr().add(HEADER_LENGTH) as *mut u8,
                        align_of::<T>(),
                    ) as *mut T;
                    T::check_bytes(ptr, &mut ()).ok()
                }
            }

            fn as_mut<T: bytecheck::CheckBytes<()>>(&mut self) -> Option<&mut T> {
                unsafe {
                    let ptr =
                        align_up(self.0.as_mut_ptr().add(HEADER_LENGTH), align_of::<T>()) as *mut T;
                    // TODO: check bounds maybe?
                    let _ = T::check_bytes(ptr, &mut ()).ok()?;
                    Some(&mut *(ptr))
                }
            }

            // Safety: Assumes there is enough room for the object plus alignment
            unsafe fn as_maybe_uninit<T>(&mut self) -> &mut MaybeUninit<T> {
                let align = align_of::<T>();
                let ptr =
                    align_up(self.0.as_mut_ptr().add(HEADER_LENGTH), align) as *mut MaybeUninit<T>;
                &mut *ptr
            }
        }
    };
}

// source: https://github.com/rust-osdev/linked-list-allocator/blob/c2aa6ec6de74cd7f1bee4a7db964ebce3d7574b6/src/lib.rs#L369
fn align_up(addr: *mut u8, align: usize) -> *mut u8 {
    let offset = addr.align_offset(align);
    addr.wrapping_add(offset)
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

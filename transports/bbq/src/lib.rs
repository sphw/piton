extern crate alloc;

use alloc::sync::Arc;
use bbqueue::{framed::FrameGrantR, BufStorage};
use core::mem::align_of;
use core::{
    marker::PhantomData,
    mem::size_of,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering},
};
use piton::{BufR as _, BufW as _, Error, ServiceRx, Yule};

pub type Storage<const N: usize = { 4096 * 4 }> = Arc<BufStorage<N>>;

pub struct Tx<const N: usize> {
    signal: Arc<AtomicUsize>,
    prod: bbqueue::framed::FrameProducer<Storage<N>>,
}

pub struct Rx<const N: usize> {
    signal: Arc<AtomicUsize>,
    cons: bbqueue::framed::FrameConsumer<Storage<N>>,
}

pub struct Server<const N: usize, Arg, Ret> {
    queue: bbqueue::BBBuffer<Storage<N>>,
    rx: Rx<N>,
    tx: Vec<Tx<N>>,
    _phantom: PhantomData<(Arg, Ret)>,
}

impl<const N: usize, Arg, Ret> Default for Server<N, Arg, Ret> {
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
            _phantom: PhantomData,
        }
    }
}

impl<const N: usize, Arg: Yule, Ret: Yule> Server<N, Arg, Ret> {
    pub fn client(&mut self) -> Client<N, Arg, Ret> {
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
            _phantom: PhantomData,
        }
    }
}

impl<const N: usize, Arg: Yule, Ret: Yule> ServiceRx for Server<N, Arg, Ret> {
    type Arg = Arg;
    type Ret = Ret;

    type Responder<'a> = Responder<'a, N, Arg, Ret>;

    type BufR<'r> = BufR<N, Self::Arg>;

    type BufW<'r> = BufW<N, Self::Ret>;

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
        let tx = &mut self.tx[id];
        let mut resp = BufW {
            grant: tx
                .prod
                .grant(size_of::<Ret>() + HEADER_LENGTH + align_of::<Ret>())
                .map_err(|_| Error::BufferUnderflow)?,
            _phantom: Default::default(),
        };
        resp.grant.fill(0);
        buf.auto_release(true);
        Ok(Some(piton::Recv {
            req: BufR::new(buf)?,
            resp,
            responder: Responder {
                signal: tx.signal.as_ref(),
                _phantom: PhantomData,
            },
        }))
    }
}

pub struct Responder<'a, const N: usize, Arg, Ret> {
    signal: &'a AtomicUsize,
    _phantom: PhantomData<(Arg, Ret)>,
}

impl<'a, const N: usize, Arg: Yule, Ret: Yule> piton::Responder for Responder<'a, N, Arg, Ret> {
    type ServerTransport = Server<N, Arg, Ret>;

    fn send(self, msg: <Self::ServerTransport as ServiceRx>::BufW<'_>) -> Result<(), Error> {
        msg.commit();
        self.signal.fetch_add(1, Ordering::Release);
        Ok(())
    }
}

pub struct Client<const N: usize, Arg, Ret> {
    id: usize,
    tx: Tx<N>,
    rx: Rx<N>,
    _phantom: PhantomData<(Arg, Ret)>,
}

impl<const N: usize, Arg: Yule, Ret: Yule> piton::ServiceTx for Client<N, Arg, Ret> {
    type Arg = Arg;
    type Ret = Ret;

    type BufW<'r> = BufW<N, Arg>;

    type BufR<'r> = BufR<N, Ret>;

    fn call<'r, 'm>(&'r mut self, mut msg: Self::BufW<'m>) -> Result<Self::BufR<'r>, Error> {
        msg.grant[0..{ size_of::<usize>() }].copy_from_slice(&self.id.to_be_bytes());
        msg.commit();
        self.tx.signal.fetch_add(1, Ordering::Release);
        let mut resp = self.rx.recv();
        resp.auto_release(true);
        BufR::new(resp)
    }

    fn alloc<'r>(&mut self) -> Result<Self::BufW<'r>, Error> {
        self.tx
            .prod
            .grant(size_of::<Arg>() + HEADER_LENGTH + align_of::<Arg>())
            .map_err(|_| Error::BufferOverflow)
            .map(|g| unsafe { BufW::new(g) })
    }
}

struct BusTx<const N: usize, Msg> {
    tx: Tx<N>,
    _phantom: PhantomData<Msg>,
}

impl<const N: usize, Msg: Yule> piton::BusTx for BusTx<N, Msg> {
    type Msg = Msg;
    type BufW<'r> = BufW<N, Self::Msg>;

    fn send(&mut self, msg: Self::BufW<'_>) -> Result<(), Error> {
        msg.commit();
        self.tx.signal.fetch_add(1, Ordering::Release);
        Ok(())
    }

    fn alloc<'r>(&mut self) -> Result<Self::BufW<'r>, Error> {
        self.tx
            .prod
            .grant(size_of::<Msg>() + align_of::<Msg>() + HEADER_LENGTH)
            .map_err(|_| Error::BufferOverflow)
            .map(|g| unsafe { BufW::new(g) })
    }
}

struct BusRx<const N: usize, Msg> {
    rx: Rx<N>,
    _phantom: PhantomData<Msg>,
}

impl<const N: usize, Msg: Yule> piton::BusRx for BusRx<N, Msg> {
    type Msg = Msg;
    type BufR<'r> = BufR<N, Msg>;

    fn recv(&mut self) -> Result<Option<Self::BufR<'_>>, Error> {
        let mut buf = self.rx.recv();
        buf.auto_release(true);
        Ok(Some(BufR::new(buf)?))
    }
}

const HEADER_LENGTH: usize = size_of::<usize>();

pub struct BufW<const N: usize, T> {
    grant: bbqueue::framed::FrameGrantW<Storage<N>>,
    _phantom: PhantomData<T>,
}

impl<const N: usize, T> BufW<N, T> {
    /// Safety: Ensure grant is zeroed before calling this method
    unsafe fn new(grant: bbqueue::framed::FrameGrantW<Storage<N>>) -> Self {
        //assert!(grant.len() >= size_of::<T>() + HEADER_LENGTH + align_of::<T>());
        Self {
            grant,
            _phantom: Default::default(),
        }
    }
}

impl<'a, const N: usize, T: piton::Yule> piton::BufW<'a, T> for BufW<N, T> {
    fn as_mut(&mut self) -> &mut T {
        let addr = self.grant.as_ptr();
        let offset = addr.align_offset(align_of::<T>()) + HEADER_LENGTH;
        // Safety: BufW's contents are validated on creation
        unsafe { T::from_mut_slice_unchecked(&mut self.grant.deref_mut()[offset..]) }
    }
}
impl<'a, const N: usize, T: piton::Yule> piton::BufR<'a, T> for BufW<N, T> {
    fn as_ref(&self) -> &T {
        let addr = self.grant.as_ptr();
        let offset = addr.align_offset(align_of::<T>()) + HEADER_LENGTH;
        // Safety: BufW's contents are validated on creation
        unsafe { T::from_slice_unchecked(&self.grant.deref()[offset..]) }
    }
}

impl<const N: usize, T: piton::Yule> DerefMut for BufW<N, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.as_mut()
    }
}
impl<const N: usize, T: piton::Yule> Deref for BufW<N, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.as_ref()
    }
}

impl<const N: usize, T: piton::Yule> Deref for BufR<N, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.as_ref()
    }
}
impl<'a, const N: usize, T: piton::Yule> piton::BufR<'a, T> for BufR<N, T> {
    fn as_ref(&self) -> &T {
        let addr = self.grant.as_ptr();
        let offset = addr.align_offset(align_of::<T>()) + HEADER_LENGTH;
        // Safety: BufW's contents are validated on creation
        unsafe { T::from_slice_unchecked(&self.grant.deref()[offset..]) }
    }
}

impl<const N: usize, T> BufW<N, T> {
    fn commit(self) {
        self.grant
            .commit(size_of::<T>() + align_of::<T>() + HEADER_LENGTH)
    }
}
pub struct BufR<const N: usize, T> {
    grant: bbqueue::framed::FrameGrantR<Storage<N>>,
    _phantom: PhantomData<T>,
}

impl<const N: usize, T: Yule> BufR<N, T> {
    fn new(grant: bbqueue::framed::FrameGrantR<Storage<N>>) -> Result<Self, Error> {
        let addr = grant.as_ptr();
        let offset = addr.align_offset(align_of::<T>()) + HEADER_LENGTH;
        if !T::validate(&grant[offset..]) {
            return Err(Error::InvalidMsg);
        }
        Ok(BufR {
            grant,
            _phantom: Default::default(),
        })
    }
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
        self.cons.read().expect("race condition")
    }
}

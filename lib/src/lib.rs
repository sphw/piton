#![no_std]
use core::{
    marker::PhantomData,
    mem::{size_of, MaybeUninit},
};

/// [`Buf`] repersents a buffer owned by a transport. Piton is designed to be used with transports
/// that allow for DMA and zero-copy. We do that by allowing the transport to allocate a buffer (usually in a ring-buf of some-sort),
/// that it owns. This trait is shared between readable and writable buffers, but often will have to be implemented seperately for both
/// ends.
pub trait Buf {
    /// Whether or not the buffer can hold type T, this should take into account alignment
    fn can_insert<T>(&self) -> bool;

    /// Check if Buf holds a valid `T`, and returns a reference
    fn as_ref<T: bytecheck::CheckBytes<()>>(&self) -> Option<&T>;

    /// Checks if Buf holds a valid `T`, and returns a mutable reference
    fn as_mut<T: bytecheck::CheckBytes<()>>(&mut self) -> Option<&mut T>;

    /// # Safety
    /// This function assumes that the caller used `can_insert` to determine if
    /// `T` will fit in the buffer. Undefined behavior abound if you call this without checking
    /// size first
    unsafe fn as_maybe_uninit<T>(&mut self) -> &mut MaybeUninit<T>;
}

pub trait ServiceTx {
    type BufW<'r>: Buf + 'r
    where
        Self: 'r;

    type BufR<'r>: Buf + 'r
    where
        Self: 'r;

    type Error;

    fn call<'r, 'm, M, R>(
        &'r mut self,
        req_type: u32,
        msg: TypedBuf<Self::BufW<'m>, M>,
    ) -> Result<TypedBuf<Self::BufR<'r>, R>, Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm,
        R: bytecheck::CheckBytes<()> + 'r;

    fn alloc<'r>(&mut self, capacity: usize) -> Result<Self::BufW<'r>, Self::Error>;
    fn alloc_typed<'r, T: bytecheck::CheckBytes<()>>(
        &mut self,
    ) -> Result<TypedBuf<Self::BufW<'r>, T>, Self::Error> {
        let buf = self.alloc(size_of::<T>() + 128)?;
        Ok(TypedBuf::new(buf).unwrap())
    }
}

pub trait ServiceRx {
    type Responder<'a>: Responder<Error = Self::Error, ServerTransport = Self> + 'a
    where
        Self: 'a;
    type BufR<'r>: Buf + 'r
    where
        Self: 'r;
    type BufW<'r>: Buf + 'r
    where
        Self: 'r;

    type Error;

    #[allow(clippy::type_complexity)]
    fn recv(
        &mut self,
    ) -> Result<Option<Recv<Self::BufW<'_>, Self::BufR<'_>, Self::Responder<'_>>>, Self::Error>;
}

pub trait BusTx {
    type BufW<'r>: Buf + 'r
    where
        Self: 'r;

    type Error;

    fn send<'r, 'm, M>(
        &'r mut self,
        req_type: u32,
        msg: TypedBuf<Self::BufW<'m>, M>,
    ) -> Result<(), Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm;

    fn alloc<'r>(&mut self, capacity: usize) -> Result<Self::BufW<'r>, Self::Error>;
    fn alloc_typed<'r, T: bytecheck::CheckBytes<()>>(
        &mut self,
    ) -> Result<TypedBuf<Self::BufW<'r>, T>, Self::Error> {
        let buf = self.alloc(size_of::<T>() + 128)?;
        Ok(TypedBuf::new(buf).unwrap())
    }
}

pub trait BusRx {
    type BufR<'r>: Buf + 'r
    where
        Self: 'r;

    type Error;

    #[allow(clippy::type_complexity)]
    fn recv(&mut self) -> Result<Option<Msg<Self::BufR<'_>>>, Self::Error>;
}

pub struct Msg<BR> {
    pub req: BR,
    pub msg_type: u32,
}

pub struct Recv<BW, BR, R> {
    pub req: BR,
    pub resp: BW,
    pub responder: R,
    pub msg_type: u32,
}

pub struct TypedBuf<B, T> {
    pub buf: B,
    _phantom: PhantomData<T>,
}

impl<T: bytecheck::CheckBytes<()>, B: Buf> TypedBuf<B, T> {
    pub fn new(buf: B) -> Option<Self> {
        if !buf.can_insert::<T>() {
            return None;
        }
        Some(Self {
            buf,
            _phantom: PhantomData,
        })
    }

    pub fn insert(&mut self, obj: T) -> InsertToken<B, T> {
        unsafe {
            self.buf.as_maybe_uninit().write(obj);
            InsertToken::new()
        }
    }

    pub fn as_ref(&self) -> Option<&T> {
        self.buf.as_ref()
    }

    pub fn as_mut(&mut self) -> Option<&mut T> {
        self.buf.as_mut()
    }
}

pub struct InsertToken<B, T>(PhantomData<(B, T)>);

impl<B, T> InsertToken<B, T> {
    unsafe fn new() -> InsertToken<B, T> {
        InsertToken(PhantomData)
    }
}

pub trait Responder {
    type ServerTransport: ServiceRx;
    type Error;

    fn send<'r, 'm, M>(
        self,
        req_type: u32,
        msg: TypedBuf<<Self::ServerTransport as ServiceRx>::BufW<'m>, M>,
    ) -> Result<(), Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm;
}

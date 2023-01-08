#![cfg_attr(not(feature = "std"), no_std)]

pub mod std;

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

/// `ServiceTx` is implemented by the sender side of a service transport. Service transports
/// have request reply or function call semantics.
///
/// Like all transports, the implementor must provide
/// buffer types that are owned by the transport.
pub trait ServiceTx {
    /// A buffer that is issued for writes
    type BufW<'r>: Buf + 'r
    where
        Self: 'r;

    /// A buffer that issued for reads, returned by `call`
    type BufR<'r>: Buf + 'r
    where
        Self: 'r;

    /// Calls the service and waits for a reply.
    fn call<'r, 'm, M, R>(
        &'r mut self,
        req_type: u32,
        msg: TypedBuf<Self::BufW<'m>, M>,
    ) -> Result<TypedBuf<Self::BufR<'r>, R>, Error>
    where
        M: bytecheck::CheckBytes<()> + 'm,
        R: bytecheck::CheckBytes<()> + 'r;

    /// Allocs a new writable buffer. Generally these buffers are owned by the transport
    fn alloc<'r>(&mut self, capacity: usize) -> Result<Self::BufW<'r>, Error>;

    /// Allocs a new writable typed buffer. Generally these buffers are owned by the transport
    fn alloc_typed<'r, T: bytecheck::CheckBytes<()>>(
        &mut self,
    ) -> Result<TypedBuf<Self::BufW<'r>, T>, Error> {
        let buf = self.alloc(size_of::<T>() + 128)?;
        Ok(TypedBuf::new(buf).unwrap())
    }
}

/// `ServiceRx` is implemented by the reciever side of a service transport
///
/// Like all transports, the implementor must provide
/// buffer types that are owned by the transport.
pub trait ServiceRx {
    /// A [`Responder`] that allow's a user to respond to a recieved message
    type Responder<'a>: Responder<ServerTransport = Self> + 'a
    where
        Self: 'a;

    /// A buffer that issued for reads, returned by `call`
    type BufR<'r>: Buf + 'r
    where
        Self: 'r;

    /// A buffer that is issued for writes
    type BufW<'r>: Buf + 'r
    where
        Self: 'r;

    /// Polls the transport for any new messages, returning [`Recv`] containg the responder, a write buffer,
    /// and the new message
    #[allow(clippy::type_complexity)]
    fn recv(
        &mut self,
    ) -> Result<Option<Recv<Self::BufW<'_>, Self::BufR<'_>, Self::Responder<'_>>>, Error>;
}

/// `BusTx` is implemented by the sender side of a service transport. Bus transports
/// do not have request-reply semantics. They act like a channel, or, as the name would imply, a bus.
///
/// Like all transports, the implementor must provide
/// buffer types that are owned by the transport.
pub trait BusTx {
    /// A buffer that is issued for writes
    type BufW<'r>: Buf + 'r
    where
        Self: 'r;

    /// Sends a message onto the bus transport
    fn send<'r, 'm, M>(
        &'r mut self,
        req_type: u32,
        msg: TypedBuf<Self::BufW<'m>, M>,
    ) -> Result<(), Error>
    where
        M: bytecheck::CheckBytes<()> + 'm;

    /// Allocs a new writable buffer. Generally these buffers are owned by the transport
    fn alloc<'r>(&mut self, capacity: usize) -> Result<Self::BufW<'r>, Error>;

    /// Allocs a new writable typed buffer. Generally these buffers are owned by the transport
    fn alloc_typed<'r, T: bytecheck::CheckBytes<()>>(
        &mut self,
    ) -> Result<TypedBuf<Self::BufW<'r>, T>, Error> {
        let buf = self.alloc(size_of::<T>() + 128)?;
        Ok(TypedBuf::new(buf).unwrap())
    }
}

/// `BusRx` is implemented by the reciever side of a bus transport
///
/// Like all transports, the implementor must provide
/// buffer types that are owned by the transport.
pub trait BusRx {
    type BufR<'r>: Buf + 'r
    where
        Self: 'r;

    #[allow(clippy::type_complexity)]
    fn recv(&mut self) -> Result<Option<Msg<Self::BufR<'_>>>, Error>;
}

/// A message received by [`BusRx`]
pub struct Msg<BR> {
    pub req: BR,
    pub msg_type: u32,
}

/// A request-reply pair received by [`ServiceRx`]
pub struct Recv<BW, BR, R> {
    pub req: BR,
    pub resp: BW,
    pub responder: R,
    pub msg_type: u32,
}

/// A wrapper around a [`Buf`] that is typed
pub struct TypedBuf<B, T> {
    pub buf: B,
    _phantom: PhantomData<T>,
}

impl<T: bytecheck::CheckBytes<()>, B: Buf> TypedBuf<B, T> {
    /// Creates a new [`TypedBuf`] from a [`Buf`].
    ///
    /// This functions returns `None` is `Buf` is not a valid `T`
    pub fn new(buf: B) -> Option<Self> {
        if !buf.can_insert::<T>() {
            return None;
        }
        Some(Self {
            buf,
            _phantom: PhantomData,
        })
    }

    /// Inserts `T` into the typed-buf, and returns a [`InsertToken`]
    pub fn insert(&mut self, obj: T) -> InsertToken<B, T> {
        unsafe {
            self.buf.as_maybe_uninit().write(obj);
            InsertToken::new()
        }
    }

    /// Returns a reference to type if the buf contains a valid `T`
    pub fn as_ref(&self) -> Option<&T> {
        self.buf.as_ref()
    }

    /// Returns a mutable reference to type if the buf contains a valid `T`
    pub fn as_mut(&mut self) -> Option<&mut T> {
        self.buf.as_mut()
    }
}

/// A insert token that repersents the result of [`TypedBuf::insert`]. This is used to force a user to write into a buffer before returning from a function
pub struct InsertToken<B, T>(PhantomData<(B, T)>);

impl<B, T> InsertToken<B, T> {
    unsafe fn new() -> InsertToken<B, T> {
        InsertToken(PhantomData)
    }
}

/// `Responder` is implemented by structs that allow a user to respond to a request.
pub trait Responder {
    /// The [`ServiceRx`] that this responder is associated with
    type ServerTransport: ServiceRx;

    /// Sends a response
    fn send<'r, 'm, M>(
        self,
        req_type: u32,
        msg: TypedBuf<<Self::ServerTransport as ServiceRx>::BufW<'m>, M>,
    ) -> Result<(), Error>
    where
        M: bytecheck::CheckBytes<()> + 'm;
}

#[derive(Debug)]
pub enum Error {
    BufferUnderflow,
    BufferOverflow,
    InvalidMsg,
    TxFail,
    RxFail,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::BufferUnderflow => write!(f, "buffer underflow"),
            Error::BufferOverflow => write!(f, "buffer overflow"),
            Error::InvalidMsg => write!(f, "invalid msg"),
            Error::TxFail => write!(f, "tx fail"),
            Error::RxFail => write!(f, "rx fail"),
        }
    }
}

#[cfg(feature = "std")]
impl ::std::error::Error for Error {}

//#![cfg_attr(not(feature = "std"), no_std)]

pub mod types;

use core::{
    mem::{align_of, size_of},
    ops::{Deref, DerefMut},
};

/// Yule is Piton's verson of the ULE (unaligned little-endian data) concept from the fantastic [`zerovec`] crate.
///
/// Piton Yules are types that can be safetly zeroed, have no padding, and let you use [`bytecheck`] to verify their contents.
/// Yules can only hold POD (plain-ole data); i.e they can't hold pointers.
///
/// # Safety
/// By implementing Yule you are guarenteeing that your type has no padding and can safetly be zeroed. If either of those things
/// are untrue, you are doing a UB.
pub unsafe trait Yule:
    bytecheck::CheckBytes<()> + Sized + Default + Clone + 'static
{
    fn validate(slice: &[u8]) -> bool {
        slice.len() >= size_of::<Self>()
            && slice.as_ptr().align_offset(align_of::<Self>()) == 0
            && unsafe { Self::check_bytes(slice.as_ptr() as *const Self, &mut ()).is_ok() }
    }

    fn from_mut_slice(slice: &mut [u8]) -> Option<&mut Self> {
        Self::validate(slice).then(|| unsafe { Self::from_mut_slice_unchecked(slice) })
    }

    fn from_slice(slice: &[u8]) -> Option<&Self> {
        Self::validate(slice).then(|| unsafe { Self::from_slice_unchecked(slice) })
    }

    ///  Creates a new a mutable reference to `Self` from a slice and does not check validity
    ///
    /// *NOTE:* This will panic in debug mode if the slice is invalid
    ///
    /// # Safety
    /// you must ensure that the slice you pass is valid as `Self`
    /// This means alignment must be valid, the slice must be as long as self, and the actual bytes repersent a valid `Self`
    unsafe fn from_mut_slice_unchecked(slice: &mut [u8]) -> &mut Self {
        debug_assert!(Self::validate(slice));
        &mut *(slice.as_mut_ptr() as *mut Self)
    }
    ///  Creates a new a reference to `Self` from a slice and does not check validity
    ///
    /// *NOTE:* This will panic in debug mode if the slice is invalid
    ///
    /// # Safety
    /// you must ensure that the slice you pass is valid as `Self`
    /// This means alignment must be valid, the slice must be as long as self, and the actual bytes repersent a valid `Self`
    unsafe fn from_slice_unchecked(slice: &[u8]) -> &Self {
        debug_assert!(Self::validate(slice));
        &*(slice.as_ptr() as *const Self)
    }

    /// Returns the struct as an byte slice
    fn as_slice(&self) -> &[u8] {
        // Safety: This is safe due to the bounds on a Yule, essentially no part of the struct shall be uninitialized bytes
        unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, size_of::<Self>()) }
    }
}

pub trait BufR<'a, T>: Deref<Target = T>
where
    T: Yule,
{
    fn as_ref(&self) -> &T;
}

pub trait BufW<'a, T>: BufR<'a, T> + DerefMut<Target = T>
where
    T: Yule,
{
    fn as_mut(&mut self) -> &mut T;
}

/// `ServiceTx` is implemented by the sender side of a service transport. Service transports
/// have request reply or function call semantics.
///
/// Like all transports, the implementor must provide
/// buffer types that are owned by the transport.
pub trait ServiceTx {
    type Arg: Yule;
    type Ret: Yule;

    /// A buffer that is issued for writes
    type BufW<'r>: BufW<'r, Self::Arg> + 'r
    where
        Self: 'r;

    /// A buffer that issued for reads, returned by `call`
    type BufR<'r>: BufR<'r, Self::Ret> + 'r
    where
        Self: 'r;

    /// Calls the service and waits for a reply.
    fn call<'r, 'm>(&'r mut self, msg: Self::BufW<'m>) -> Result<Self::BufR<'r>, Error>;

    /// Allocs a new writable buffer. Generally these buffers are owned by the transport
    fn alloc<'r>(&mut self) -> Result<Self::BufW<'r>, Error>;
}

/// `ServiceRx` is implemented by the reciever side of a service transport
///
/// Like all transports, the implementor must provide
/// buffer types that are owned by the transport.
pub trait ServiceRx {
    type Arg: Yule;
    type Ret: Yule;

    /// A [`Responder`] that allow's a user to respond to a recieved message
    type Responder<'a>: Responder<ServerTransport = Self> + 'a
    where
        Self: 'a;

    /// A buffer that issued for reads, returned by `call`
    type BufR<'r>: BufR<'r, Self::Arg> + 'r
    where
        Self: 'r;

    /// A buffer that is issued for writes
    type BufW<'r>: BufW<'r, Self::Ret> + 'r
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
    type Msg: Yule;
    /// A buffer that is issued for writes
    type BufW<'r>: BufW<'r, Self::Msg> + 'r
    where
        Self: 'r;

    /// Sends a message onto the bus transport
    fn send(&'_ mut self, msg: Self::BufW<'_>) -> Result<(), Error>;

    /// Allocs a new writable buffer. Generally these buffers are owned by the transport
    fn alloc<'r>(&mut self) -> Result<Self::BufW<'r>, Error>;
}

/// `BusRx` is implemented by the reciever side of a bus transport
///
/// Like all transports, the implementor must provide
/// buffer types that are owned by the transport.
pub trait BusRx {
    type Msg: Yule;
    type BufR<'r>: BufR<'r, Self::Msg> + 'r
    where
        Self: 'r;

    #[allow(clippy::type_complexity)]
    fn recv(&mut self) -> Result<Option<Self::BufR<'_>>, Error>;
}

/// A request-reply pair received by [`ServiceRx`]
pub struct Recv<BW, BR, R> {
    pub req: BR,
    pub resp: BW,
    pub responder: R,
}

/// `Responder` is implemented by structs that allow a user to respond to a request.
pub trait Responder {
    /// The [`ServiceRx`] that this responder is associated with
    type ServerTransport: ServiceRx;

    /// Sends a response
    fn send(self, msg: <Self::ServerTransport as ServiceRx>::BufW<'_>) -> Result<(), Error>;
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

#[derive(Clone, bytecheck::CheckBytes, PartialEq, Eq, Hash)]
pub struct ZeroPad<const N: usize> {
    _pad: [u8; N],
}

impl<const N: usize> core::fmt::Debug for ZeroPad<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ZeroPad").finish()
    }
}

impl<const N: usize> Default for ZeroPad<N> {
    fn default() -> Self {
        Self { _pad: [0; N] }
    }
}

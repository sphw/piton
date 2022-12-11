#![no_std]
use core::mem::MaybeUninit;

pub trait Buf {
    type Error;
    type InsertToken<T>;

    fn insert<T>(&mut self, obj: T) -> Result<Self::InsertToken<T>, Self::Error>;
    fn as_maybe_uninit<T>(&mut self) -> &mut MaybeUninit<T>;

    fn as_ref<T: bytecheck::CheckBytes<()>>(&self) -> Option<&T>;
    fn as_mut<T: bytecheck::CheckBytes<()>>(&mut self) -> Option<&mut T>;
}

pub trait TypedBuf<T: bytecheck::CheckBytes<()>> {
    type Buf: Buf;
    type InsertToken;

    fn insert(&mut self, obj: T) -> Self::InsertToken;
    fn as_maybe_uninit(&mut self) -> &mut MaybeUninit<T>;
    fn as_ref(&self) -> Option<&T>;
    fn as_mut(&mut self) -> Option<&mut T>;

    fn from_buf(buf: Self::Buf) -> Result<Self, <Self::Buf as Buf>::Error>
    where
        Self: Sized;
}

pub trait ClientTransport {
    type Buf<'r, T: bytecheck::CheckBytes<()>>: TypedBuf<T> + 'r
    where
        T: 'r;
    type Error;

    fn call<'r, 'm, M, R>(
        &'r mut self,
        req_type: u32,
        msg: Self::Buf<'m, M>,
    ) -> Result<Self::Buf<'r, R>, Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm,
        R: bytecheck::CheckBytes<()> + 'r;

    fn send<'r, 'm, M, R>(
        &'r mut self,
        req_type: u32,
        msg: Self::Buf<'m, M>,
    ) -> Result<(), Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm;

    fn alloc<'r, T>(&mut self) -> Result<Self::Buf<'r, T>, Self::Error>
    where
        T: bytecheck::CheckBytes<()>;
}

pub type InsertToken<'r, T, M> =
    <<T as ServerTransport>::TypedBuf<'r, M> as TypedBuf<M>>::InsertToken;

pub trait ServerTransport {
    type Responder<'a>: Responder<Error = Self::Error, ServerTransport = Self> + 'a
    where
        Self: 'a;
    type Buf<'r>: Buf<Error = Self::Error> + 'r
    where
        Self: 'r;
    type TypedBuf<'r, M>: TypedBuf<M, Buf = Self::Buf<'r>> + 'r
    where
        M: bytecheck::CheckBytes<()> + 'r,
        Self: 'r;
    type Error;

    #[allow(clippy::type_complexity)]
    fn recv(&mut self) -> Result<Option<Recv<Self::Buf<'_>, Self::Responder<'_>>>, Self::Error>;
}

pub struct Recv<B, R> {
    pub req: B,
    pub resp: B,
    pub responder: R,
    pub msg_type: u32,
}

pub trait Responder {
    type ServerTransport: ServerTransport;
    type Error;

    fn send<'r, 'm, M>(
        self,
        req_type: u32,
        msg: <Self::ServerTransport as ServerTransport>::TypedBuf<'m, M>,
    ) -> Result<(), Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm;
}

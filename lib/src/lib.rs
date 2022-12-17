#![no_std]
use core::{
    marker::PhantomData,
    mem::{size_of, MaybeUninit},
};

pub trait Buf {
    fn can_insert<T>(&self) -> bool;

    fn as_ref<T: bytecheck::CheckBytes<()>>(&self) -> Option<&T>;
    fn as_mut<T: bytecheck::CheckBytes<()>>(&mut self) -> Option<&mut T>;

    /// # Safety
    /// This function assumes that the caller used `can_insert` to determine if
    /// `T` will fit in the buffer. Undefined behavior abound if you call this without checking
    /// size first
    unsafe fn as_maybe_uninit<T>(&mut self) -> &mut MaybeUninit<T>;
}

pub trait ClientTransport {
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

pub trait ServerTransport {
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

pub struct Recv<BW, BR, R> {
    pub req: BR,
    pub resp: BW,
    pub responder: R,
    pub msg_type: u32,
}

pub trait Responder {
    type ServerTransport: ServerTransport;
    type Error;

    fn send<'r, 'm, M>(
        self,
        req_type: u32,
        msg: TypedBuf<<Self::ServerTransport as ServerTransport>::BufW<'m>, M>,
    ) -> Result<(), Self::Error>
    where
        M: bytecheck::CheckBytes<()> + 'm;
}

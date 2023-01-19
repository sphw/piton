use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use multiqueue::{BroadcastReceiver, BroadcastSender};
use piton::{BufR, BufW, Error, Yule};

pub fn pair<Msg: Yule>(capacity: u64) -> (BusTx<Msg>, BusRx<Msg>) {
    let (tx, rx) = multiqueue::broadcast_queue(capacity);
    (
        BusTx {
            tx,
            _phantom: PhantomData,
        },
        BusRx {
            rx,
            _phantom: PhantomData,
        },
    )
}

#[derive(Clone)]
pub struct BusTx<Msg: Yule> {
    tx: BroadcastSender<Buf<Msg>>,
    _phantom: PhantomData<Msg>,
}

impl<T: Yule> piton::BusTx for BusTx<T> {
    type Msg = T;
    type BufW<'r> = Buf<T>;

    fn send<'r, 'm>(&'r mut self, msg: Self::BufW<'m>) -> Result<(), Error> {
        self.tx.try_send(msg).map_err(|e| {
            println!("{:?}", e);
            Error::TxFail
        })
    }

    fn alloc<'r>(&mut self) -> Result<Self::BufW<'r>, Error> {
        Ok(Buf(T::default()))
    }
}

pub struct BusRx<Msg: Yule> {
    rx: BroadcastReceiver<Buf<Msg>>,
    _phantom: PhantomData<Msg>,
}

impl<Msg: Yule> Clone for BusRx<Msg> {
    fn clone(&self) -> Self {
        Self {
            rx: self.rx.add_stream(),
            _phantom: PhantomData,
        }
    }
}

impl<Msg: Yule> piton::BusRx for BusRx<Msg> {
    type Msg = Msg;
    type BufR<'r> = Buf<Msg>;

    fn recv(&mut self) -> Result<Option<Self::BufR<'_>>, Error> {
        let buf = self.rx.recv().map_err(|_| Error::RxFail)?;
        Ok(Some(buf))
    }
}

#[derive(Clone)]
pub struct Buf<T: Yule>(T);

impl<T: Yule> BufW<'_, T> for Buf<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Yule> BufR<'_, T> for Buf<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: Yule> DerefMut for Buf<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Yule> Deref for Buf<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

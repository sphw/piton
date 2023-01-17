use std::{
    alloc::Layout,
    mem::{align_of, size_of, MaybeUninit},
};

use multiqueue::{BroadcastReceiver, BroadcastSender};
use piton::{Buf, Error, Msg};

pub fn pair(capacity: u64) -> (BusTx, BusRx) {
    let (tx, rx) = multiqueue::broadcast_queue(capacity);
    (BusTx { tx }, BusRx { rx })
}

#[derive(Clone)]
pub struct BusTx {
    tx: BroadcastSender<BufW>,
}

impl piton::BusTx for BusTx {
    type BufW<'r> = BufW;

    fn send<'r, 'm, M>(
        &'r mut self,
        req_type: u32,
        mut msg: piton::TypedBuf<Self::BufW<'m>, M>,
    ) -> Result<(), Error>
    where
        M: bytecheck::CheckBytes<()> + 'm,
    {
        if let Some(ref mut inner) = msg.buf.inner {
            inner[{ size_of::<usize>() }..HEADER_LENGTH].copy_from_slice(&req_type.to_be_bytes());
        }
        self.tx.try_send(msg.buf).map_err(|e| {
            println!("{:?}", e);
            Error::TxFail
        })
    }

    fn alloc<'r>(&mut self, _capacity: usize) -> Result<Self::BufW<'r>, Error> {
        Ok(BufW { inner: None })
    }
}

pub struct BusRx {
    rx: BroadcastReceiver<BufW>,
}

impl Clone for BusRx {
    fn clone(&self) -> Self {
        Self {
            rx: self.rx.add_stream(),
        }
    }
}

impl piton::BusRx for BusRx {
    type BufR<'r> = BufW;

    fn recv(&mut self) -> Result<Option<piton::Msg<Self::BufR<'_>>>, Error> {
        let buf = self.rx.recv().map_err(|_| Error::RxFail)?;
        let Some(inner) = &buf.inner else {
            return Ok(None);
        };
        let msg_type = u32::from_be_bytes(
            inner[{ size_of::<usize>() }..HEADER_LENGTH]
                .try_into()
                .map_err(|_| Error::BufferUnderflow)?,
        );

        Ok(Some(Msg { req: buf, msg_type }))
    }
}

#[derive(Clone)]
pub struct BufW {
    inner: Option<Box<[u8]>>,
}

const HEADER_LENGTH: usize = 4 + size_of::<usize>();

impl Buf for BufW {
    fn can_insert<T>(&self) -> bool {
        true
    }

    fn as_ref<T: bytecheck::CheckBytes<()>>(&self) -> Option<&T> {
        match &self.inner {
            Some(inner) => {
                let addr = unsafe { inner.as_ptr().add(HEADER_LENGTH) } as *const u8;
                let offset = addr.align_offset(align_of::<T>());
                if inner.len() < offset {
                    return None;
                }
                let ptr = addr.wrapping_add(offset) as *const T;
                unsafe { T::check_bytes(ptr, &mut ()).ok() }
            }
            None => None,
        }
    }

    fn as_mut<T: bytecheck::CheckBytes<()>>(&mut self) -> Option<&mut T> {
        match &self.inner {
            Some(inner) => {
                let addr = unsafe { inner.as_ptr().add(HEADER_LENGTH) } as *const u8;
                let offset = addr.align_offset(align_of::<T>());
                if inner.len() < offset {
                    return None;
                }
                let ptr = addr.wrapping_add(offset) as *mut T;
                unsafe {
                    T::check_bytes(ptr, &mut ()).ok()?;
                    Some(&mut *ptr)
                }
            }
            None => None,
        }
    }

    unsafe fn as_maybe_uninit<T>(&mut self) -> &mut std::mem::MaybeUninit<T> {
        let len = align_of::<T>() + size_of::<T>() + HEADER_LENGTH;
        let buf = self.inner.insert(alloc_box_buffer(len));
        let ptr = buf.as_mut_ptr().add(HEADER_LENGTH);
        let offset = ptr.align_offset(align_of::<T>());
        unsafe { &mut *(ptr.add(offset) as *mut MaybeUninit<T>) }
    }
}

fn alloc_box_buffer(len: usize) -> Box<[u8]> {
    if len == 0 {
        return <Box<[u8]>>::default();
    }
    let layout = Layout::array::<u8>(len).expect("overflow");
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    let slice_ptr = core::ptr::slice_from_raw_parts_mut(ptr, len);
    unsafe { Box::from_raw(slice_ptr) }
}

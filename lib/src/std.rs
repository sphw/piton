use core::{
    mem::{self, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr::addr_of,
    slice,
};

use bytecheck::{CheckBytes, StructCheckError};

#[derive(Debug)]
pub struct Vec<T, const N: usize> {
    len: u64,
    buf: [MaybeUninit<T>; N],
}

impl<C, T: CheckBytes<C>, const N: usize> CheckBytes<C> for Vec<T, N> {
    type Error = StructCheckError;
    unsafe fn check_bytes<'__bytecheck>(
        value: *const Self,
        context: &mut C,
    ) -> ::core::result::Result<&'__bytecheck Self, StructCheckError> {
        <u64 as CheckBytes<C>>::check_bytes(addr_of!((*value).len), context)
            .map_err(|_e| StructCheckError { field_name: "len" })?;
        let bytes = (*value).buf.as_ptr() as *const T;
        for index in 0..N {
            let el_bytes = bytes.add(index);
            T::check_bytes(el_bytes, context)
                .map_err(|_| StructCheckError { field_name: "buf" })?;
        }
        Ok(&*value)
    }
}

impl<T: PartialEq, const N: usize> PartialEq for Vec<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T, const N: usize> Vec<T, N> {
    const ELEM: MaybeUninit<T> = MaybeUninit::uninit();
    const INIT: [MaybeUninit<T>; N] = [Self::ELEM; N];

    pub const fn new() -> Self {
        Self {
            buf: Self::INIT,
            len: 0,
        }
    }

    pub fn push(&mut self, elem: T) -> Result<(), T> {
        if self.len >= (N as u64) {
            return Err(elem);
        }
        self.buf[self.len as usize].write(elem);
        self.len += 1;
        Ok(())
    }

    pub fn push_unchecked(&mut self, elem: T) {
        self.buf[self.len as usize].write(elem);
        self.len += 1;
    }

    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.buf.as_ptr() as *const T, self.len as usize) }
    }
}

impl<T, const N: usize> Default for Vec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Deref for Vec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T, const N: usize> DerefMut for Vec<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.buf.as_mut_ptr() as *mut T, self.len as usize) }
    }
}

impl<T: Clone, const N: usize> Clone for Vec<T, N> {
    fn clone(&self) -> Self {
        let mut new = Self::new();
        for elem in &self[..] {
            new.push_unchecked(elem.clone());
        }
        new
    }
}

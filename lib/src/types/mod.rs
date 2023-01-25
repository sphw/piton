mod vec;
pub use vec::*;
pub mod le;
pub use le::*;

macro_rules! primative_yule_impl {
    ($prim:ident) => {
        unsafe impl crate::Yule for $prim {}
    };
}

primative_yule_impl! { u64le }
primative_yule_impl! { u32le }
primative_yule_impl! { u16le }
primative_yule_impl! { u8 }
primative_yule_impl! { i64le }
primative_yule_impl! { i32le }
primative_yule_impl! { i16le }
primative_yule_impl! { i8 }
primative_yule_impl! { f64 }
primative_yule_impl! { f32 }
primative_yule_impl! { bool }

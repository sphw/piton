// based on https://docs.rs/nora_endian/0.1.2/src/nora_endian/lib.rs.html
// rehomed here so we can implement bytecheck and force consistent alignment
use core::{fmt, num::*, ops::*};

macro_rules! ety {
    ($name:ident, $ty:ty, $trait:ident.$fn:ident, $traitas:ident.$fnas:ident) => {
        impl $trait<Self> for $name {
            type Output = Self;

            fn $fn(self, rhs: Self) -> Self {
                Self::from(<$ty>::from(self).$fn(<$ty>::from(rhs)))
            }
        }

        impl $trait<$ty> for $name {
            type Output = Self;

            fn $fn(self, rhs: $ty) -> Self {
                Self::from(<$ty>::from(self).$fn(rhs))
            }
        }

        impl $trait<$name> for $ty {
            type Output = Self;

            fn $fn(self, rhs: $name) -> Self {
                self.$fn(Self::from(rhs))
            }
        }

        impl $traitas<Self> for $name {
            fn $fnas(&mut self, rhs: Self) {
                *self = self.$fn(rhs)
            }
        }

        impl $traitas<$ty> for $name {
            fn $fnas(&mut self, rhs: $ty) {
                *self = self.$fn(rhs)
            }
        }

        impl $traitas<$name> for $ty {
            fn $fnas(&mut self, rhs: $name) {
                *self = self.$fn(rhs)
            }
        }
    };
    (tyonly $ty:ty, $name:ident, $to:ident, $align:literal) => {
        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C, align($align))]
        pub struct $name($ty);

        impl PartialEq<$ty> for $name {
            fn eq(&self, rhs: &$ty) -> bool {
                <$ty>::from(*self).eq(rhs)
            }
        }

        impl PartialEq<$name> for $ty {
            fn eq(&self, rhs: &$name) -> bool {
                self.eq(&<$ty>::from(*rhs))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                <$ty>::from(self.0).fmt(f)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                <$ty>::from(self.0).fmt(f)
            }
        }

        impl fmt::LowerHex for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                <$ty>::from(self.0).fmt(f)
            }
        }

        impl fmt::UpperHex for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                <$ty>::from(self.0).fmt(f)
            }
        }
    };
    ($ty:ty, $name:ident, $to:ident, $align:literal) => {
        ety!(tyonly $ty, $name, $to, $align);

        impl $name {
            pub const fn new(value: $ty) -> Self {
                Self(value.$to())
            }
        }

        impl From<$ty> for $name {
            fn from(value: $ty) -> Self {
                Self(value.$to())
            }
        }

        impl From<$name> for $ty {
            fn from(value: $name) -> Self {
                value.0.$to()
            }
        }

        impl Default for $name {
            #[inline(always)]
            fn default() -> Self {
                Self(0)
            }
        }

        ety!($name, $ty, Add.add, AddAssign.add_assign);
        ety!($name, $ty, Sub.sub, SubAssign.sub_assign);
        ety!($name, $ty, Mul.mul, MulAssign.mul_assign);
        ety!($name, $ty, Div.div, DivAssign.div_assign);
        ety!($name, $ty, Rem.rem, RemAssign.rem_assign);
        ety!($name, $ty, BitOr.bitor, BitOrAssign.bitor_assign);
        ety!($name, $ty, BitAnd.bitand, BitAndAssign.bitand_assign);
        ety!($name, $ty, BitXor.bitxor, BitXorAssign.bitxor_assign);

        impl<C: ?Sized> bytecheck::CheckBytes<C> for $name {
            type Error = core::convert::Infallible;

            #[inline]
            unsafe fn check_bytes<'a>(
                value: *const Self,
                _: &mut C,
            ) -> Result<&'a Self, Self::Error> {
                Ok(&*value)
            }
        }

        impl Not for $name {
            type Output = Self;

            fn not(self) -> Self {
                Self(self.0.not())
            }
        }
    };
    (nz $zty:ident, $ty:ty, $name:ident, $to:ident, $align:literal) => {
        ety!(tyonly $ty, $name, $to, $align);

        impl $name {
            pub const fn new(value: $ty) -> Self {
                Self(match <$ty>::new(value.get().$to()) {
                    Some(v) => v,
                    _ => unreachable!(),
                })
            }

            pub const fn get(&self) -> $zty {
                $zty(self.0.get())
            }
        }

        impl From<$ty> for $name {
            fn from(value: $ty) -> Self {
                Self(<$ty>::new(value.get().$to()).unwrap())
            }
        }

        impl From<$name> for $ty {
            fn from(value: $name) -> Self {
                Self::new(value.0.get().$to()).unwrap()
            }
        }
    };
    (be $ty:ty, $name:ident, $align:literal) => {
        ety!($ty, $name, to_be, $align);
    };
    (le $ty:ty, $name:ident, $align:literal) => {
        ety!($ty, $name, to_le, $align);
    };
    (nz be $zty:ident, $ty:ty, $name:ident, $align:literal) => {
        ety!(nz $zty, $ty, $name, to_be, $align);
    };
    (nz le $zty:ident, $ty:ty, $name:ident, $align:literal) => {
        ety!(nz $zty, $ty, $name, to_le, $align);
    };
}

ety!(be u16, u16be, 2);
ety!(be u32, u32be, 4);
ety!(be u64, u64be, 8);
ety!(le u16, u16le, 2);
ety!(le u32, u32le, 4);
ety!(le u64, u64le, 8);

ety!(be i16, i16be, 2);
ety!(be i32, i32be, 4);
ety!(be i64, i64be, 8);
ety!(le i16, i16le, 2);
ety!(le i32, i32le, 4);
ety!(le i64, i64le, 8);

ety!(nz be u16be, NonZeroU16, NonZeroU16be, 2);
ety!(nz be u32be, NonZeroU32, NonZeroU32be, 4);
ety!(nz be u64be, NonZeroU64, NonZeroU64be, 8);
ety!(nz le u16le, NonZeroU16, NonZeroU16le, 2);
ety!(nz le u32le, NonZeroU32, NonZeroU32le, 4);
ety!(nz le u64le, NonZeroU64, NonZeroU64le, 8);

ety!(nz be i16be, NonZeroI16, NonZeroI16be, 2);
ety!(nz be i32be, NonZeroI32, NonZeroI32be, 4);
ety!(nz be i64be, NonZeroI64, NonZeroI64be, 8);
ety!(nz le i16le, NonZeroI16, NonZeroI16le, 2);
ety!(nz le i32le, NonZeroI32, NonZeroI32le, 4);
ety!(nz le i64le, NonZeroI64, NonZeroI64le, 8);

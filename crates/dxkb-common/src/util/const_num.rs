use zerocopy::{FromBytes, Immutable, IntoBytes};

macro_rules! const_num_impl {
    ($tname:ident, $ttraitname:ident, $ntype:ty) => {
        pub trait $ttraitname {
            const N: $ntype;
        }

        #[derive(Clone, Copy, FromBytes, IntoBytes, Immutable, PartialEq, Eq, PartialOrd, Ord)]
        #[repr(transparent)]
        pub struct $tname<const N: $ntype>($ntype);
        impl<const N: $ntype> $tname<N> {
            pub const fn new() -> Self {
                $tname(N)
            }
        }

        impl<const N: $ntype> $ttraitname for $tname<N> {
            const N: $ntype = N;
        }


    };
}

const_num_impl!(ConstU8, ConstU8Like, u8);
const_num_impl!(ConstU16, ConstU16Like, u16);
const_num_impl!(ConstU32, ConstU32Like, u32);
const_num_impl!(ConstU64, ConstU64Like, u64);
const_num_impl!(ConstU128, ConstU128Like, u128);

const_num_impl!(ConstI8, ConstI8Like, i8);
const_num_impl!(ConstI16, ConstI16Like, i16);
const_num_impl!(ConstI32, ConstI32Like, i32);
const_num_impl!(ConstI64, ConstI64Like, i64);
const_num_impl!(ConstI128, ConstI128Like, i128);

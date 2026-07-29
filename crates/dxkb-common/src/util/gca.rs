pub const U8_AS_USIZE<const N: u8>: usize = N as usize;
pub const USIZE_ADD<const A: usize, const B: usize>: usize = A + B;
pub const SIZE_OF<T>: usize = size_of::<T>();

#[macro_export]
macro_rules! usize_add_n {
    ($n:expr) => {
        ::dxkb_common::util::gca::USIZE_ADD::<0, $n>
    };

    ($n:expr, $($ns:expr),*) => {
        ::dxkb_common::util::gca::USIZE_ADD::<$n, {$crate::usize_add_n!($($ns),*)}>
    };
}

use zerocopy::{FromBytes, Immutable, IntoBytes};

use super::{ConstCond, IsTrue};

// trait BitArraySize<const N: usize> {
//     const SIZE: usize = 1 + ((N - 1) / 8);
// }
//
pub struct BitArraySize<const N: usize>;

impl<const N: usize> BitArraySize<N> {
    const SIZE: usize = 1 + ((N - 1) / 8);
}

#[repr(transparent)]
#[derive(Clone, FromBytes, IntoBytes, Immutable, Debug)]
pub struct BitArray<const N: usize> where [(); BitArraySize::<N>::SIZE]: {
    buf: [u8; BitArraySize::<N>::SIZE]
}

impl<const N: usize> BitArray<N> where [(); BitArraySize::<N>::SIZE]: {
    pub const fn new() -> Self {
        Self {
            buf: [0; BitArraySize::<N>::SIZE]
        }
    }

    fn assert_within_bounds(index: usize) {
        assert!(index < N, "Index out of bounds: {}", index);
    }

    /// Sets the value of a bit to 1. [`true`] will be returning if the value actually changed. Otherwise false will be returned.
    #[inline]
    pub unsafe fn set_unchecked(&mut self, index: usize) -> bool {
        let value = unsafe { self.buf.get_unchecked_mut(index / 8) };
        let copy = *value;
        *value |= 1 << (index % 8);
        return copy != *value
    }

    pub unsafe fn toggle_unchecked(&mut self, index: usize) {
        unsafe {
            *self.buf.get_unchecked_mut(index / 8) ^= 1 << (index % 8);
        }
    }

    /// Sets the value of a bit to 1. [`true`] will be returning if the value actually changed. Otherwise false will be returned.
    pub unsafe fn clear_unchecked(&mut self, index: usize) -> bool {
        let value = unsafe { self.buf.get_unchecked_mut(index / 8) };
        let copy = *value;
        *value &= !(1 << (index % 8));
        return copy != *value
    }

    pub unsafe fn put_unchecked(&mut self, index: usize, value: bool) {
        unsafe {
            if value {
                self.set_unchecked(index);
            } else {
                self.clear_unchecked(index);
            }
        }
    }

    pub unsafe fn get_unchecked(&self, index: usize) -> bool {
        unsafe {
            (*self.buf.get_unchecked(index / 8) & (1 << (index % 8))) != 0
        }
    }

    fn set_infallible<const I: usize>(&mut self) where ConstCond<{I < N}>: IsTrue {
        unsafe {
            // SAFETY: Bounds checked in type assertions
            self.set_unchecked(I);
        }
    }

    fn clear_infallible<const I: usize>(&mut self) where ConstCond<{I < N}>: IsTrue {
        unsafe {
            // SAFETY: Bounds checked in type assertions
            self.clear_unchecked(I);
        }
    }

    fn toggle_infallible<const I: usize>(&mut self) where ConstCond<{I < N}>: IsTrue {
        unsafe {
            // SAFETY: Bounds checked in type assertions
            self.toggle_unchecked(I);
        }
    }

    fn put_infallible<const I: usize>(&mut self, value: bool) where ConstCond<{I < N}>: IsTrue {
        unsafe {
            // SAFETY: Bounds checked in type assertions
            self.put_unchecked(I, value);
        }
    }

    fn get_infallible<const I: usize>(&self) -> bool where ConstCond<{I < N}>: IsTrue {
        unsafe {
            // SAFETY: Bounds checked in type assertions
            self.get_unchecked(I)
        }
    }

    #[inline]
    pub fn set(&mut self, index: usize) -> bool {
        Self::assert_within_bounds(index);
        unsafe {
            // SAFETY: Bounds previously checked.
            self.set_unchecked(index)
        }
    }

    #[inline]
    pub fn clear(&mut self, index: usize) -> bool {
        Self::assert_within_bounds(index);
        unsafe {
            // SAFETY: Bounds previously checked.
            self.clear_unchecked(index)
        }
    }

    #[inline]
    pub fn toggle(&mut self, index: usize) {
        Self::assert_within_bounds(index);
        unsafe {
            // SAFETY: Bounds previously checked.
            self.toggle_unchecked(index);
        }
    }

    #[inline]
    pub fn put(&mut self, index: usize, value: bool) {
        Self::assert_within_bounds(index);
        unsafe {
            // SAFETY: Bounds previously checked.
            self.put_unchecked(index, value);
        }
    }

    #[inline]
    pub fn get(&self, index: usize) -> bool {
        Self::assert_within_bounds(index);
        unsafe {
            // SAFETY: Bounds previously checked.
            self.get_unchecked(index)
        }
    }
}

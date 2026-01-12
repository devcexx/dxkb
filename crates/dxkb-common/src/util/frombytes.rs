use core::mem::MaybeUninit;

use super::{ConstCond, IsTrue};

/// Marks a type as it is possible to be constructed from an arbitrary byte
/// array. The implementation of this trait is unsafe because the implementing
/// type must ensure that any possible byte array whose size is at least the
/// size of the type, can be interpreted as a valid instance of such type.
///
/// This is used instead of zerocopy traits, because zerocopy conversion traits
/// may fail if the input size is less than the type size. On this trait, this
/// is checked at compile time by requesting a reference to an array that has
/// the minimum required bytes.
pub unsafe trait FromBytesSized: Sized {
    const SELF_SIZE: usize = size_of::<Self>();
    unsafe fn from_raw_ptr(ptr: *const u8) -> *const Self {
        unsafe {
            core::mem::transmute(ptr)
        }
    }

    unsafe fn from_raw_ptr_mut(ptr: *mut u8) -> *mut Self {
        unsafe {
            core::mem::transmute(ptr)
        }
    }
}

pub trait FromByteArray {
    fn ref_from_byte_array<'a, const N: usize>(arr: &'a[u8; N]) -> &'a Self where Self: FromBytesSized, ConstCond<{N >= Self::SELF_SIZE}>: IsTrue {
        unsafe {
            let ptr = Self::from_raw_ptr(arr.as_ptr());
            &*ptr
        }
    }

    fn mut_from_byte_array<'a, const N: usize>(arr: &'a mut [u8; N]) -> &'a mut Self where Self: FromBytesSized, ConstCond<{N >= Self::SELF_SIZE}>: IsTrue {
        unsafe {
            let ptr = Self::from_raw_ptr_mut(arr.as_mut_ptr());
            &mut *ptr
        }
    }

    fn from_byte_array<'a, const N: usize>(arr: &'a[u8; N]) -> Self where Self: FromBytesSized, ConstCond<{N >= Self::SELF_SIZE}>: IsTrue {
        unsafe {
            core::mem::transmute_copy(&*Self::from_raw_ptr(arr.as_ptr()))
        }
    }
}

impl<T> FromByteArray for T {}

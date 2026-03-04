use core::mem::ManuallyDrop;

pub fn array_initialize<T, F, const N: usize>(f: F) -> [T; N]
where
    F: Fn(usize) -> T,
{
    let mut arr: [T; N] = unsafe {
        // SAFETY: Must fill all elements of the array before returning it.
        core::mem::MaybeUninit::uninit().assume_init()
    };
    for i in 0..N {
        arr[i] = f(i);
    }

    arr
}

/**
 * Unifies the const generic type N1 into N2, safely casting an array of [T; N1]
 * into [T; N2]. This function is used to circumvent situations in which the
 * Rust compiler cannot unify between two const generic expressions that are
 * different in appearance but results in the same value. (sample compiler
 * issue: https://github.com/rust-lang/rust/issues/153393).
 */
#[inline(always)]
pub const fn array_unify_length<T, const N1: usize, const N2: usize>(arr: [T; N1]) -> [T; N2] {
    const fn assert_array_length_equal<const N1: usize, const N2: usize>() {
        assert!(N1 == N2, "Array lengths must be the same to unify them");
    }

    const { assert_array_length_equal::<N1, N2>() };

    unsafe {
        // SAFETY: N1 == N2, and both src and dst arrays are of type T
        let arr = ManuallyDrop::new(arr);
        arr.as_ptr().cast::<[T; N2]>().read()
    }
}

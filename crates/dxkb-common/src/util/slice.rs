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

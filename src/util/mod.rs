/// Runs a interrupt-free code block, taking a mutable reference from
/// the given [`core::cell::UnsafeCell`] at the beginning of the free
/// block, making sure the creation of these mutable references only
/// happens once in the lifetime of the free block. it is * UNSAFE *
/// to call this macro from itself, so please don't do it.
#[macro_export]
macro_rules! free_with_muts {
    ($($ident:ident <- $ref:expr),*, |$cs:ident| $block:block) => {
        cortex_m::interrupt::free(|cs| {
            $(
               let $ident = unsafe { &mut *($ref).borrow(cs).get() };
            )*

            let $cs = cs;

            $block
        })
    };
}

#![no_std]
#![feature(generic_const_items, min_generic_const_args, generic_const_args)]
#![feature(exhaustive_patterns)]
#![feature(const_trait_impl)]
#![feature(const_convert)]
#![feature(min_adt_const_params)]
#![allow(incomplete_features)]


pub mod bus;
mod devlog;
mod key;
pub mod time;
pub mod util;

pub use key::*;

pub use log as __log;

#[macro_export]
macro_rules! diff_wrapped {
    ($max:expr, $newer:expr, $older:expr) => {
        if ($newer) > ($older) {
            ($newer) - ($older)
        } else {
            (($max) + 1) - ($older) + ($newer)
        }
    };
}

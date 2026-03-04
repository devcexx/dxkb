#![no_std]
#![feature(exhaustive_patterns)]
#![feature(generic_const_exprs)]
#![feature(const_trait_impl)]
#![feature(const_convert)]
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

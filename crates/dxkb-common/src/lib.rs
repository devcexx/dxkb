#![no_std]

pub mod bus;
pub mod time;
mod devlog;
pub mod util;
mod key;
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

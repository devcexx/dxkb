#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr_concat)]
#![feature(maybe_uninit_uninit_array_transpose)]
#![feature(macro_metavar_expr)]
#![no_std]

pub mod hid;
pub mod keyboard;
pub mod keys;
pub mod log;
pub mod usb;
pub mod debug;

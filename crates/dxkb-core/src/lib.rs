#![allow(incomplete_features)]
#![feature(macro_metavar_expr_concat)]
#![feature(maybe_uninit_uninit_array_transpose)]
#![feature(macro_metavar_expr)]
#![feature(inherent_associated_types)]
#![feature(generic_const_items, min_generic_const_args, generic_const_args)]
#![no_std]

pub mod hid;
pub mod keyboard;
pub mod keys;
pub mod log;
pub mod usb;
pub mod debug;

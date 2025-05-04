#![no_std]
#![no_main]
#![allow(incomplete_features)]
#![allow(static_mut_refs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::bare_urls)]
#![feature(generic_const_exprs)]

use dxkb_core::keyboard::{Left, Right};

#[cfg(not(any(feature = "side-right", feature = "side-left")))]
compile_error!("Not side has been specified. Either side-left or side-right feature must be enabled!");

#[cfg(all(feature = "side-right", feature = "side-left"))]
compile_error!("Only side-left or side-right features must be eanbled at a time!");

#[cfg(feature = "side-left")]
pub type CurrentSide = Left;

#[cfg(feature = "side-right")]
pub type CurrentSide = Right;

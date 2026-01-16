#![no_std]
#![no_main]
#![allow(incomplete_features)]
#![allow(static_mut_refs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::bare_urls)]
#![feature(generic_const_exprs)]

use core::marker::PhantomData;

use dxkb_core::keyboard::{Left, PinMasterSense};
use stm32f4xx_hal::{gpio::PinPull, hal::digital::InputPin};

#[cfg(not(any(feature = "side-right", feature = "side-left")))]
compile_error!(
    "Not side has been specified. Either side-left or side-right feature must be enabled!"
);

#[cfg(all(feature = "side-right", feature = "side-left"))]
compile_error!("Only side-left or side-right features must be enabled at a time!");

#[cfg(all(feature = "usb-force-master", feature = "usb-force-slave"))]
compile_error!("Only usb-force-master or usb-force-slave features must be enabled at a time!");

pub trait UseTypeLike {
    type Target;
}

pub struct UseType<P, T> {
    _p: PhantomData<P>,
    _t: PhantomData<T>,
}

impl<P, T> UseTypeLike for UseType<P, T> {
    type Target = T;
}

#[cfg(feature = "side-left")]
pub type CurrentSide = Left;

#[cfg(feature = "side-right")]
pub type CurrentSide = Right;

#[cfg(feature = "usb-force-master")]
pub type MasterCheckType<P> = <UseType<P, AlwaysMaster> as UseTypeLike>::Target;

#[cfg(feature = "usb-force-slave")]
pub type MasterCheckType<P> = <UseType<P, AlwaysSlave> as UseTypeLike>::Target;

#[cfg(not(any(feature = "usb-force-master", feature = "usb-force-slave")))]
pub type MasterCheckType<P> = PinMasterSense<P>;

pub fn make_usb_master_checker<P: InputPin + PinPull>(p: P) -> MasterCheckType<P> {
    #[cfg(feature = "usb-force-master")]
    return AlwaysMaster;

    #[cfg(feature = "usb-force-slave")]
    return AlwaysSlave;

    #[cfg(not(any(feature = "usb-force-master", feature = "usb-force-slave")))]
    return PinMasterSense::new(p);
}

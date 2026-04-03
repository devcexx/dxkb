#![no_std]
#![no_main]
#![allow(static_mut_refs)]
#![feature(generic_const_exprs)]
#![feature(const_index)]
#![feature(const_trait_impl)]
#![feature(core_intrinsics)]
#![feature(macro_metavar_expr)]
use core::intrinsics::black_box;
use core::marker::PhantomData;
use core::mem::MaybeUninit;

use cortex_m::asm;
use cortex_m::delay::Delay;
use cortex_m_rt::entry;
use dxkb_common::dev_info;
use dxkb_peripheral::pin_set::PinSet;
use stm32f4xx_hal::gpio::{DynamicPin, Output, Pin, PinMode, PushPull};
use stm32f4xx_hal::hal::digital::OutputPin;
use stm32f4xx_hal::pac::gpioa::OTYPER;
use stm32f4xx_hal::pac::gpioa::moder::MODER0;
use stm32f4xx_hal::pac::gpioa::otyper::OT0;
use stm32f4xx_hal::pac::gpioa::pupdr::PUPDR0;
use stm32f4xx_hal::pac::stk::val;
use stm32f4xx_hal::pac::{GPIOA, GPIOB, GPIOC, GPIOD, NVIC, RCC, SYSCFG};
use stm32f4xx_hal::{pac, rcc::RccExt};
use stm32f4xx_hal::{
    prelude::*,
};
use stm32f4xx_hal::interrupt;
#[allow(unused_imports)]
use panic_itm as _;




#[entry]
fn main() -> ! {
    unsafe {
            main0()
    }

}

fn enable_comp_cell() {
    unsafe {
        RCC::steal()
    }.apb2enr().modify(|_, w| w.syscfgen().enabled());

    let z = unsafe { SYSCFG::steal().cmpcr().as_ptr() };


    unsafe {
        z.write(z.read() | 0b1);
    }
    dev_info!("Waiting until compensation cell is ready");

    while unsafe { z.read() } & 0x100 == 0 {
    }
    dev_info!("Comp cell ready");
}

/*
 *
 type KeyMatrixRowPins = (
     Pin<'B', 10, Output<PushPull>>,
     Pin<'B', 2, Output<PushPull>>,
     Pin<'B', 1, Output<PushPull>>,
 );

 type KeyMatrixColPins = (
     Pin<'A', 6, Input>,
     Pin<'A', 5, Input>,
     Pin<'A', 4, Input>,
     Pin<'A', 3, Input>,
     Pin<'A', 2, Input>,
 );
 */

static mut DETECT_PIN: MaybeUninit<Pin<'A', 3, Output<PushPull>>> = MaybeUninit::uninit();
static mut INP_PIN: MaybeUninit<Pin<'A', 10>> = MaybeUninit::uninit();

#[inline(always)]
fn toggle_pin(p: &mut Pin<'B', 10, Output<PushPull>>) {
    p.toggle();
}

#[inline(never)]
fn main_loop(p: &mut Pin<'B', 10, Output<PushPull>>) -> ! {
    p.set_speed(stm32f4xx_hal::gpio::Speed::VeryHigh);
    loop {
        unsafe {
            let x = GPIOB::steal().odr().read().bits();
            GPIOB::steal().odr().write(|y| y.bits(!x));
        }
        unsafe {
            core::arch::asm! {
                "nop",
                "nop",
                "nop",
                "nop",
                "nop",
                "nop",
                "nop",
                "nop",
                "nop",
                "nop"
            }
        }
    }
}

#[inline(never)]
fn do_pin_change<P: PinSet>(p: &mut P, x: bool) {
    p.write_all(x);
}


#[inline(never)]
fn do_shit<P: PinSet>(p: &mut P) {
    p.write_single(0, true);
}

#[inline(never)]
fn do_pin_change2<const P1: char, const N1: u8, M1: PinMode, const P2: char, const N2: u8, M2: PinMode>(p: Pin<P1, N1, M1>, p2: Pin<P2, N2, M2>) {
    p.into_push_pull_output();
    p2.into_push_pull_output();
}


unsafe fn main0() -> ! {
    let mut dp = pac::Peripherals::take().unwrap();
    let mut cortex = cortex_m::Peripherals::take().unwrap();

    let rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(25.MHz())
        .sysclk(96.MHz())
        .pclk1(48.MHz())
        .pclk2(48.MHz())
        .freeze();

    let gpioa = dp.GPIOA.split();
    let gpiob = dp.GPIOB.split();

    itm_logger::init_with_level(log::Level::Trace).unwrap();
    //RingBufferLogger::install(unsafe { &HID_LOGGER }).unwrap();

    dev_info!("Device startup");
    dev_info!("Clock info: {:?}", clocks);
    let mut delay = Delay::new(cortex.SYST, 96_000_000);

    let mut p1 = gpioa.pa8.into_push_pull_output();
    let mut p2 = gpiob.pb5.into_push_pull_output();

    unsafe {
        dev_info!("GPIOA_MODER: {:b}", GPIOA::steal().moder().read().bits());
        dev_info!("GPIOA_OTYPER: {:b}", GPIOA::steal().otyper().read().bits());
    }

    let mut pin_set = (p1.into_dynamic(), p2.into_dynamic());

    pin_set.make_output_push_pull();
    do_shit(&mut pin_set);

    let mut value = true;
    loop {
       pin_set.write_single(0, value);
       unsafe {
           core::arch::asm! {
               "nop",
               "nop",
               "nop",
               "nop",
               "nop",
               "nop",
               "nop",
               "nop",
               "nop",
               "nop"
           }
       }
        value = !value;
    }
//    main_loop(&mut prow1);
}



// struct PinSet2<const P1: char, const N1: u8, M1, const P2: char, const N2: u8, M2> {
//     p1: Pin<P1, N1, M1>,
//     p2: Pin<P2, N2, M2>,
// }



// impl<const P1: char, const N1: u8, M1, const P2: char, const N2: u8, M2> PinSet2<P1, N1, M1, P2, N2, M2> {
//     const HAS_GPIOA_PINS: bool = has_pin_in_port(&[P1, P2], 'A');
//     const HAS_GPIOB_PINS: bool = has_pin_in_port(&[P1, P2], 'B');
//     const HAS_GPIOC_PINS: bool = has_pin_in_port(&[P1, P2], 'C');
//     const HAS_GPIOD_PINS: bool = has_pin_in_port(&[P1, P2], 'D');

//     const GPIOA_MODER_OUTPUT_VALUE: u32 = gpio_moder_value_for_pins(&[P1, P2], &[N1, N2], 0b01, 'A');
//     const GPIOA_MODER_BITMASK: u32 = gpio_moder_bitmask(&[P1, P2], &[N1, N2], 'A');

//     const GPIOB_MODER_OUTPUT_VALUE: u32 = gpio_moder_value_for_pins(&[P1, P2], &[N1, N2], 0b01, 'B');
//     const GPIOB_MODER_BITMASK: u32 = gpio_moder_bitmask(&[P1, P2], &[N1, N2], 'B');

//     const GPIOC_MODER_OUTPUT_VALUE: u32 = gpio_moder_value_for_pins(&[P1, P2], &[N1, N2], 0b01, 'C');
//     const GPIOC_MODER_BITMASK: u32 = gpio_moder_bitmask(&[P1, P2], &[N1, N2], 'C');

//     const GPIOD_MODER_OUTPUT_VALUE: u32 = gpio_moder_value_for_pins(&[P1, P2], &[N1, N2], 0b01, 'D');
//     const GPIOD_MODER_BITMASK: u32 = gpio_moder_bitmask(&[P1, P2], &[N1, N2], 'D');
// }
//
//

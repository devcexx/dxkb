#![no_std]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use stm32f4xx_hal::pac::Interrupt;

pub mod clock;
pub mod gpio;
pub mod key_matrix;
pub mod uart_dma_rb;
pub mod usart;
pub mod dma;

pub trait InterruptReceiver {
    const INTERRUPT: Interrupt;
}

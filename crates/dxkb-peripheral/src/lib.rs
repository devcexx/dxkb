#![no_std]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use cortex_m::asm::bootload;
use dxkb_common::dev_info;
use stm32f4xx_hal::{pac::{Interrupt, PWR, RCC, RTC, SCB}, rcc::Enable};

pub mod clock;
pub mod gpio;
pub mod key_matrix;
pub mod uart_dma_rb;
pub mod usart;
pub mod dma;

pub trait InterruptReceiver {
    const INTERRUPT: Interrupt;
}


// At this point I'm not sure where to put this
pub struct BootloaderUtil;

#[cfg(feature = "stm32f411")]
impl BootloaderUtil {
    fn set_bootloader_enter_request(enable: bool) {
        let rtc = unsafe {
            RTC::steal()
        };

        // PWR peripheral clock must be enabled before accessing the PWR registers.
        unsafe {
            PWR::enable_unchecked();
        };

        let pwr = unsafe {
            PWR::steal()
        };

        // Remove write protection for RTC registers
        pwr.cr().modify(|_, w| {
            w.dbp().set_bit()
        });

        rtc.bkp0r().modify(|r, w| {
            let mut val = r.bkp().bits();
            if enable {
                val |= 1;
            } else {
                val &= 0xfffe;
            }
            w.bkp().set(val)
        });
    }

    fn bootloader_enter_requested() -> bool {
        let rcc = unsafe {
            RCC::steal()
        };

        let rtc = unsafe {
            RTC::steal()
        };

        // Enable the PWR peripheral clock — required before accessing backup domain
        rcc.apb1enr().modify(|_, w| w.pwren().set_bit());

        (rtc.bkp0r().read().bkp().bits() & 0x1) > 0
    }

    /// STM32F411 system memory base address (built-in USB DFU bootloader).
    const SYSTEM_MEMORY_BASE: u32 = 0x1FFF_0000;

    // SAFETY: This must be called early in the boot process, before any configuration to the peripherals have been done (clock, etc)
    pub unsafe fn handle_bootloader_enter_request() {
        if Self::bootloader_enter_requested() {
            Self::set_bootloader_enter_request(false);
            unsafe {
                bootload(Self::SYSTEM_MEMORY_BASE as *const u32);
            }
        }
    }

    pub fn enter_bootloader() -> ! {
        Self::set_bootloader_enter_request(true);
        SCB::sys_reset();
    }
}

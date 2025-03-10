// This DXKB version is experimental and subject to deep changes in
// the future. This version should include the bare minimum for
// running a split keyboard, including USB support, USB remote wake
// up, key matrix, layout definition, and a communication port between
// the two parts of the keyboard.

//TODO Test in the future to integrate with RTIC for better handling
//interrupts? https://github.com/rtic-rs/rtic

#![no_std]
#![no_main]
#![allow(incomplete_features)]
#![allow(static_mut_refs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::bare_urls)]
#![feature(generic_const_exprs)] // I'm sorry, I just want to do some basic math with const types.

mod hid;
mod keyboard;

use core::arch::asm;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;
use core::time::Duration;

use cortex_m::delay::Delay;
use dxkb_common::bus::BusWrite;
use dxkb_common::time::{Clock, TimeDiff};
use dxkb_common::dev_info;
use hid::KeyboardPageCode;
use dxkb_peripheral::key_matrix::{
    ColumnScan, DebouncerEagerPerKey, IntoInputPinsWithSamePort, KeyMatrix, KeyState,
};

#[allow(unused_imports)]
use panic_itm as _;

use cortex_m_rt::entry;
use dxkb_peripheral::uart_dma_rb::{DmaRingBuffer, UartDmaRb};
use dxkb_split_link::{DefaultSplitLinkTimings, SplitBus, TestingTimings};
use stm32f4xx_hal::dma::{Stream5, Stream7};
use stm32f4xx_hal::gpio::{Output, Pin};
use stm32f4xx_hal::pac::{Interrupt, DMA2};
use stm32f4xx_hal::{dma::StreamsTuple, interrupt, otg_fs::USB, pac::{self, DWT, NVIC, OTG_FS_DEVICE}, prelude::*, rcc::RccExt};
use synopsys_usb_otg::UsbBus;
use usb_device::{
    device::{StringDescriptors, UsbDeviceBuilder, UsbDeviceState, UsbVidPid},
    LangID,
};
use usbd_hid::{
    descriptor::KeyboardReport,
    hid_class::{
        HIDClass, HidClassSettings, HidCountryCode, HidProtocol, HidSubClass, ProtocolModeConfig,
    },
};

type UartBus = UartDmaRb<pac::USART1, Stream7<DMA2>, Stream5<DMA2>, 4, 4, 256, 128>;

static mut EP_MEMORY: [u32; 1024] = [0; 1024];
static mut SPLIT_BUS_BUF: DmaRingBuffer<256, 128> = DmaRingBuffer::new();
static mut DMA_UART_TX_BUF: [u8; 256] = [0u8; 256];
static mut SPLIT_BUS: MaybeUninit<SplitBus<u8, TestingTimings, UartBus, DWTClock,  32>> = MaybeUninit::uninit();
static mut INTR_PIN: Pin<'B', 8, Output> = unsafe {
    core::mem::zeroed()
};


pub struct DWTClock {
    clock_freq: u32
}

#[derive(Clone, Copy)]
pub struct DWTInstant {
    cycles: u32,
}

impl DWTClock {
    fn cycles_to_nanos(&self, cycles: u32) -> u64 {
        cycles as u64 * 1_000_000_000u64 / self.clock_freq as u64
    }
}

impl Clock for DWTClock {
    type TInstant = DWTInstant;

    fn current_instant(&self) -> Self::TInstant {
        DWTInstant {
            cycles: DWT::cycle_count(),
        }
    }

    fn diff(&self, newer: Self::TInstant, older: Self::TInstant) -> TimeDiff {
        let d = newer.cycles.wrapping_sub(older.cycles) as i32;
        if d >= 0 {
            TimeDiff::Forward(Duration::from_nanos(self.cycles_to_nanos(d as u32)))
        } else {
            TimeDiff::Backward(Duration::from_nanos(self.cycles_to_nanos((-1 * d) as u32)))
        }
    }

    fn nanos(&self, instant: Self::TInstant) -> u64 {
        self.cycles_to_nanos(instant.cycles)
    }
}

#[entry]
fn main() -> ! {
    main0()
}

fn main0() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut cortex = cortex_m::Peripherals::take().unwrap();
    cortex.DWT.enable_cycle_counter();
    let rcc = dp.RCC.constrain();

    let clocks = rcc
        .cfgr
        .use_hse(25.MHz()) /* My WeAct BlackPill has a 25MHz external clock attached. Change to match your config! */
        .sysclk(96.MHz()) /* Sysclk is set to 96 MHz, so PLL for usb devices can be set to exactly 48MHz */
        .pclk1(48.MHz())
        .pclk2(48.MHz())
        .require_pll48clk()
        .freeze();

    let gpioa = dp.GPIOA.split();
    let gpiob = dp.GPIOB.split();
    let gpioc = dp.GPIOC.split();



    let mut suspend_led = gpioc.pc13.into_push_pull_output();


    itm_logger::init_with_level(log::Level::Trace).unwrap();
    dev_info!("Device startup");

    const ROWS: u8 = 4;
    const COLS: u8 = 4;

    let mut delay = Delay::new(cortex.SYST, 96_000_000);

    let debouncer = DebouncerEagerPerKey::<ROWS, COLS, 50>::new();
    let rows = (
        gpioa.pa1.into_input(),
        gpioa.pa2.into_input(),
        gpioa.pa3.into_input(),
        gpioa.pa4.into_input(),
    ).into_input_pins_with_same_port();

    let cols = (
        gpioa.pa5.into_push_pull_output(),
        gpioa.pa6.into_push_pull_output(),
        gpioa.pa7.into_push_pull_output(),
        gpiob.pb0.into_push_pull_output(),
    );

    let mut matrix: KeyMatrix<ROWS, COLS, _, _, ColumnScan, _> =
        KeyMatrix::new(clocks.sysclk(), rows, cols, debouncer);

    #[rustfmt::skip]
    let layout = [
        KeyboardPageCode::One, KeyboardPageCode::Two, KeyboardPageCode::Three, KeyboardPageCode::Four,
        KeyboardPageCode::Q, KeyboardPageCode::W, KeyboardPageCode::E, KeyboardPageCode::R,
        KeyboardPageCode::A, KeyboardPageCode::S, KeyboardPageCode::D, KeyboardPageCode::F,
        KeyboardPageCode::Z, KeyboardPageCode::X, KeyboardPageCode::C, KeyboardPageCode::V,
    ];

    let usb = USB {
        usb_global: dp.OTG_FS_GLOBAL,
        usb_device: dp.OTG_FS_DEVICE,
        usb_pwrclk: dp.OTG_FS_PWRCLK,
        pin_dm: gpioa.pa11.into(),
        pin_dp: gpioa.pa12.into(),
        hclk: clocks.hclk(),
    };

    let rx = gpiob.pb7.into_alternate();
    let tx = gpiob.pb6.into_alternate();

    let dma2 = StreamsTuple::new(dp.DMA2);
    let dma2_2 = StreamsTuple::new(unsafe {
        DMA2::steal()
    });

    //SplitBus::init(dp.USART2, todo!(), dma2.7, dma2.5, &clocks);
    // Worked with 2995200 baud
    // let usart1 = Serial::new(
    //     dp.USART1,
    //     (tx, rx),
    //     Config::default()
    //         .baudrate(9600.bps())
    //         .parity_none()
    //         .stopbits(StopBits::STOP1)
    //         .dma(DmaConfig::TxRx),
    //     &clocks).unwrap();

    unsafe {
//        NVIC::unmask(Interrupt::DMA2_STREAM7);
//        NVIC::unmask(Interrupt::DMA2_STREAM4);
    }





    // loop {
    //     let mut last_ndr = 999;
    //     unsafe {
    //         usart_dma.write_dma(b"HOLA", None).unwrap();
    //     }
    //     while !dma2_2.7.is_transfer_complete() {
    //         let cur_ndr = dma2_2.7.number_of_transfers();
    //         if last_ndr != cur_ndr {
    //             dev_info!("N: {}", cur_ndr);
    //             last_ndr = cur_ndr;
    //         }
    //     }
    //     suspend_led.toggle();
    //     delay.delay_ms(1000);
    //     usart_dma.handle_dma_interrupt();
    //     usart_dma.handle_dma_interrupt();
    // }


    let bus_allocator = UsbBus::new(usb, unsafe { addr_of_mut!(EP_MEMORY).as_mut().unwrap() });

    // TODO NKRO support
    // TODO HID standard requires GET_REPORT to be supported by
    // keyboards. However, usbd-hid is not providing support for this
    // right now. Shall I try to support it?

    // TODO Make USART NVIC interrupts a priority in which they cannot be preempted by any other interrupt.

    const NZXT_HUE2_DESCRIPTOR: [u8; 34] = [0x06, 0x72, 0xFF, 0x09, 0xA1, 0xA1, 0x01, 0x09, 0x10, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x40, 0x81, 0x02, 0x09, 0x11, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x40, 0x91, 0x02, 0xC0];
    let mut kbd_hid = HIDClass::new_ep_in_with_settings(
        &bus_allocator,
        &NZXT_HUE2_DESCRIPTOR,
        1,
        HidClassSettings {
            subclass: HidSubClass::NoSubClass,
            protocol: HidProtocol::Keyboard,
            config: ProtocolModeConfig::DefaultBehavior,
            locale: HidCountryCode::Spanish,
        },
    );

    let mut usb_dev = UsbDeviceBuilder::new(&bus_allocator, UsbVidPid(0x16c0, 0x27db))
        .device_class(0x3) // HID Device
        .device_sub_class(HidSubClass::NoSubClass as u8) // No subclass
        .device_protocol(HidProtocol::Generic as u8)
        .usb_rev(usb_device::device::UsbRev::Usb200)
        .strings(&[StringDescriptors::new(LangID::ES)
            .serial_number("0")
            .manufacturer("Dobetito")
            .product("DXKB Lily58L")])
        .unwrap()
        .supports_remote_wakeup(true)
        .build();

    let mut prev_usb_status: Option<UsbDeviceState> = None;

    let mut last_led_change_ticks = DWT::cycle_count();

    let mut uart_dma = UartDmaRb::init(dp.USART1, (tx, rx), dma2.7, dma2.5, unsafe{ &mut DMA_UART_TX_BUF }, unsafe { &mut SPLIT_BUS_BUF }, &clocks);

    let split_bus = unsafe {
        INTR_PIN = gpiob.pb8.into_push_pull_output();
        INTR_PIN.set_high();

        let sb = SPLIT_BUS.write(SplitBus::new(uart_dma,
            DWTClock {
                clock_freq: clocks.sysclk().raw(),
            }
        ));

        NVIC::unmask(Interrupt::USART1);
        sb
    };
    loop {

        let mut count = 0;
        split_bus.poll(|frame| {
            count+=1;

            dev_info!("Received frame: {:?}", frame);
        });


        let cur_usb_status = Some(usb_dev.state());
        if prev_usb_status != cur_usb_status {
            dev_info!("USB device status changed to {:?}; Remote wakeup? {}", usb_dev.state(), usb_dev.remote_wakeup_enabled());
            prev_usb_status = Some(usb_dev.state());
        }

        if usb_dev.poll(&mut [&mut kbd_hid]) {
            let mut report_buf = [0u8; 64];
            if let Ok(_report) = kbd_hid.pull_raw_report(&mut report_buf) {

                let _ = kbd_hid.push_raw_input(&report_buf).unwrap();
            }
        }

        let changed = matrix.scan_matrix();
        if usb_dev.state() == UsbDeviceState::Suspend {
            if usb_dev.remote_wakeup_enabled() {
                // Just a led animation for testing
                let elapsed_ms = (DWT::cycle_count() - last_led_change_ticks) as u64 * 1000 / clocks.hclk().raw() as u64;
                if elapsed_ms > 300 {
                    last_led_change_ticks = DWT::cycle_count();
                    suspend_led.toggle();
                }

                if changed {
                    suspend_led.set_high();
                    let dv = unsafe {
                        OTG_FS_DEVICE::steal()
                    };
                    dv.dctl().modify(|_, w| {
                        w.rwusig().set_bit()
                    });
                    delay.delay_ms(5); // According to datasheet, bit needs to be turned on between 1 and 15 ms.
                    dv.dctl().modify(|_, w| {
                        w.rwusig().clear_bit()
                    });
                }
            } else {
                suspend_led.set_high();
            }

        } else {
            suspend_led.set_high();
            let mut report = KeyboardReport::default();

            let mut next_index = 0;
            'out: for col in 0..4 {
                for row in 0..4 {
                    if next_index >= 6 {
                        break 'out;
                    }

                    if matrix.get_key_state(row, col) == KeyState::Pressed {
                        report.keycodes[next_index] = layout[row as usize * 4 + col as usize] as u8;
                        next_index += 1;
                    }
                }
            }

           // let _ = kbd_hid.push_input(&report);
        }
    }
}

#[interrupt]
fn USART1() {
    unsafe {
        INTR_PIN.set_low();
        asm!(
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
        );
        INTR_PIN.set_high();
    }
    let split_bus = unsafe {
        SPLIT_BUS.assume_init_mut()
    };

    split_bus.bus().handle_usart_intr();
    unsafe {
        INTR_PIN.set_low();
        asm!(
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
            "and r1,r1",
        );
        INTR_PIN.set_high();
    }

}

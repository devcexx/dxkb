#![no_std]
#![no_main]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)] // I'm sorry, I just want to do some basic math with const types.

mod periph;
mod util;
mod keyboard;
mod hid;

use core::ptr::addr_of_mut;

use cortex_m::delay::Delay;
use hid::KeyboardPageCode;
use periph::key_matrix::{DebouncerEagerPerKey, IntoKeyMatrixInputPinsWithSamePort, KeyMatrix, KeyState};

#[allow(unused_imports)]
use panic_itm as _;

use cortex_m_rt::entry;
use stm32f4xx_hal::{dwt::DwtExt, otg_fs::USB, pac, prelude::*, rcc::RccExt};
use synopsys_usb_otg::UsbBus;
use usb_device::{device::{StringDescriptors, UsbDeviceBuilder, UsbVidPid}, LangID};
use usbd_hid::{descriptor::{KeyboardReport, SerializedDescriptor}, hid_class::{HIDClass, HidClassSettings, HidCountryCode, HidProtocol, HidProtocolMode, HidSubClass, ProtocolModeConfig}, UsbError};

static mut EP_MEMORY: [u32; 1024] = [0; 1024];

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

    #[cfg(feature = "dev-log")]
    {
        itm_logger::init_with_level(log::Level::Info).unwrap();
    }
    dev_info!("Device startup");

    let debouncer = DebouncerEagerPerKey::<4, 4, 50>::new();
    let mut matrix: KeyMatrix<4, 4, _, _, _> = KeyMatrix::new(clocks.sysclk(), (
        gpioa.pa1.into_pull_down_input(),
        gpioa.pa2.into_pull_down_input(),
        gpioa.pa3.into_pull_down_input(),
        gpioa.pa4.into_pull_down_input(),
    ).into_input_pins_with_same_port(), (gpioa.pa5.into_push_pull_output(), gpioa.pa6.into_push_pull_output(), gpioa.pa7.into_push_pull_output(), gpiob.pb0.into_push_pull_output()), debouncer);


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

    let bus_allocator = UsbBus::new(usb, unsafe { addr_of_mut!(EP_MEMORY).as_mut().unwrap() });

    // TODO NKRO support
    // TODO HID standard requires GET_REPORT to be supported by
    // keyboards. However, usbd-hid is not providing support for this
    // right now. Shall I try to support it?

    let mut kbd_hid = HIDClass::new_ep_in_with_settings(&bus_allocator, KeyboardReport::desc(), 1, HidClassSettings {
        subclass: HidSubClass::NoSubClass,
        protocol: HidProtocol::Keyboard,
        config: ProtocolModeConfig::DefaultBehavior,
        locale: HidCountryCode::Spanish }
    );

    let mut usb_dev = UsbDeviceBuilder::new(&bus_allocator, UsbVidPid(0x16c0, 0x27db))
        .device_class(0x3) // HID Device
        .device_sub_class(HidSubClass::NoSubClass as u8) // No subclass
        .device_protocol(HidProtocol::Keyboard as u8)
        .usb_rev(usb_device::device::UsbRev::Usb200)
        .strings(&[StringDescriptors::new(LangID::ES)
            .serial_number("0")
            .manufacturer("Dobetito")
            .product("DXKB Lily58L")])
        .unwrap()
        .build();


    let mut delay = Delay::new(cortex.SYST, 96_000_000);

    loop {
        if usb_dev.poll(&mut [&mut kbd_hid]) {
            let mut report_buf = [0u8; 64];
            if let Ok(report) = kbd_hid.pull_raw_report(&mut report_buf) {
                dev_info!("Received report!");
            }

        }

        matrix.scan_matrix();


        let mut report = KeyboardReport::default();

        let mut next_index = 0;
        'out: for col in 0..4 {
            for row in 0..4 {
                if next_index >= 6 {
                    break 'out
                }

                if matrix.get_key_state(row, col) == KeyState::Pressed {
                    report.keycodes[next_index] = layout[row as usize * 4 + col as usize] as u8;
                    next_index += 1;
                }
            }
        }
        kbd_hid.push_input(&report);
    }
}

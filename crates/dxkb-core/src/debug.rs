use dxkb_common::{dev_info, dev_warn};
use dxkb_peripheral::BootloaderUtil;
use usb_device::{bus::{UsbBus, UsbBusAllocator}, device::UsbDevice};
use usbd_hid::hid_class::{HIDClass, HidClassSettings};

use crate::{log::RingBufferLogger, usb::UsbFeature};

const DEBUG_EP_DESCRIPTOR: [u8; 23] = [
    0x0b, 0x00, 0x00, 0x00, 0x00,  // USAGE (Generic Desktop:Undefined)
    0x06, 0x00, 0xff,              // USAGE_PAGE (Vendor Defined Page 1)
    0xa1, 0x01,                    // COLLECTION (Application)
    0x75, 0x08,                    //   REPORT_SIZE (8)
    0x95, 0x40,                    //   REPORT_COUNT (64)
    0x81, 0x02,                    //   INPUT (Data,Var,Abs)
    0x75, 0x08,                    //   REPORT_SIZE (8)
    0x95, 0x40,                    //   REPORT_COUNT (64)
    0x91, 0x02,                    //   OUTPUT (Data,Var,Abs)
    0xc0                           // END_COLLECTION
];

pub trait DebugRead {
    fn peek(&mut self, buf: &mut [u8]) -> usize;
    fn consume(&mut self, n: usize);
}

impl<const N: usize> DebugRead for &RingBufferLogger<N> {
    fn peek(&mut self, buf: &mut [u8]) -> usize {
        self.read_pending_bytes(buf)
    }

    fn consume(&mut self, n: usize) {
        self.drop_pending_bytes(n);
    }
}

pub struct NopDebugRead;

impl DebugRead for NopDebugRead {
    fn peek(&mut self, _buf: &mut [u8]) -> usize {
        0
    }

    fn consume(&mut self, _n: usize) {}
}


pub struct DebugHidFeature<'a, B: UsbBus, O: DebugRead> {
    hid: HIDClass<'a, B>,
    output_src: O,
    enter_bootloader: bool
}

impl <'a, B: UsbBus, O: DebugRead> DebugHidFeature<'a, B, O> {
    pub fn new(alloc: &'a UsbBusAllocator<B>, output_src: O) -> Self {
        let debug_ep = HIDClass::new_ep_in_with_settings(
            alloc,
            &DEBUG_EP_DESCRIPTOR,
            1,
            HidClassSettings::default(),
        );

        Self {
            hid: debug_ep,
            output_src,
            enter_bootloader: false
        }
    }
}

impl<'a, B: UsbBus, O: DebugRead> UsbFeature<B> for DebugHidFeature<'a, B, O> {
    const EP: usize = 1;
    type TPoll = ();

    fn endpoints_mut(&mut self) -> [&mut dyn usb_device::class::UsbClass<B>; 1] {
        [&mut self.hid]
    }

    fn usb_poll(&mut self, _device: &mut UsbDevice<B>) -> Self::TPoll {
        if self.enter_bootloader {
            BootloaderUtil::enter_bootloader();
        }

        let mut debug_buf: [u8; 64] = [0u8; 64];
        let count = self.output_src.peek(&mut debug_buf);
        if count > 0 {
            let r = self.hid.push_raw_input(&debug_buf[0..count]);
            if let Ok(_) = r {
                self.output_src.consume(count);
            }
        }

        // TODO Eventually would be nice to use a library like embedded-cli or
        // noshell, so we can make more generic the command registration and
        // handling. (Probably noshell would be a better idea, because
        // embedded-cli is more meant to read a byte at a time for autocomplete,
        // which probably we don't need.)
        if let Ok(info) = self.hid.pull_raw_report(&mut debug_buf) {
            if &debug_buf[0..info.len] == b"enter-dfu" || &debug_buf[0..info.len] == b"enter-dfu\n" {
                dev_info!("Requested entering into DFU mode...");
                // Delay the bootloader entry until the end of the poll, so that the response to the debug request can be sent back to the host before the device reboots.
                self.enter_bootloader = true;
            } else {
                dev_warn!("Ignored unknown debug request: {:02x?}", &debug_buf[0..info.len]);
            }
        }
    }
}

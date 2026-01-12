use core::fmt::{Display, Write};

use dxkb_common::{dev_info, util::{BitArray, FromByteArray, FromBytesSized}};
use usb_device::{
    bus::{UsbBus, UsbBusAllocator},
    device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid},
};
use usbd_hid::{
    descriptor::{gen_hid_descriptor, KeyboardReport, KeyboardUsage, MediaKey, SerializedDescriptor},
    hid_class::{
        HIDClass, HidClassSettings, HidCountryCode, HidProtocol, HidSubClass, ProtocolModeConfig,
    }, UsbError,
};
use zerocopy::{Immutable, IntoBytes};

pub enum KeyboardKeyChangeError {
    Unsupported,
    InvalidState,
}

impl From<UsbError> for KeyboardPollError {
    fn from(value: UsbError) -> Self {
        Self::UsbError(value)
    }
}

#[derive(Debug)]
pub enum KeyboardPollError {
    UsbError(UsbError),
    UnknownReport(u8),
}

impl Display for KeyboardPollError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            KeyboardPollError::UsbError(usb_error) => {
                write!(f, "Usb error: {:?}", usb_error)
            },
            KeyboardPollError::UnknownReport(id) => {
                write!(f, "Unknown report recv from host: {:?}", id)
            }
        }
    }
}

pub struct BasicKeyboardSettings<'s, 'b> {
    pub vid_pid: UsbVidPid,
    pub string_descriptors: &'s [StringDescriptors<'b>],
    pub poll_ms: u8,
}

/// A type that is capable of syncing the keyboard status with the USB host. Whatever USB implementation, endpoints or keyboard protocol uses under the hood us implementation-specific.
pub trait HidKeyboard {
    fn press_key(&mut self, key: KeyboardUsage) -> Result<(), KeyboardKeyChangeError>;
    fn release_key(&mut self, key: KeyboardUsage) -> Result<(), KeyboardKeyChangeError>;

    fn press_media_key(&mut self, key: MediaKey) -> Result<(), KeyboardKeyChangeError>;
    fn release_media_key(&mut self, key: MediaKey) -> Result<(), KeyboardKeyChangeError>;

    fn poll(&mut self) -> Result<bool, KeyboardPollError>;

    // TODO leds
}

const REPORT_HID_USAGE_MIN: KeyboardUsage = KeyboardUsage::KeyboardPOSTFail;
const REPORT_HID_USAGE_MAX: KeyboardUsage = KeyboardUsage::KeyboardLeftShift;
const REPORT_HID_USAGE_COUNT: usize = (REPORT_HID_USAGE_MAX as usize) - (REPORT_HID_USAGE_MIN as usize) + 1;

// Always try to read the maximum packet size allowed in USB 2 spec. usbd-hid
// crate hardcodes 64 as maximum packet size for USB endpoint, and the
// synopsys-usb-otg won't let you to read or write more than a data packet per transaction so... yeah, 64.
const USB_HID_READ_LEN: usize = 64;

const _: () = assert!(REPORT_HID_USAGE_COUNT % 8 == 0, "Report protocol HID keyboard keys usage counts that are not divisible by 8 are not supported at this point");

type ReportHidKeyboardUsageBitArray = BitArray<REPORT_HID_USAGE_COUNT>;

mod report_id {
    use zerocopy::{FromBytes, Immutable, IntoBytes};
    pub trait ReportIdLike {
        const ID: u8;
    }

    #[derive(Clone, Copy, FromBytes, IntoBytes, Immutable, PartialEq, Eq, PartialOrd, Ord)]
    #[repr(transparent)]
    pub struct ReportId<const ID: u8>(u8);
    impl<const ID: u8> ReportId<ID> {
        pub fn new() -> Self {
            ReportId(ID)
        }
    }

    impl<const ID: u8> ReportIdLike for ReportId<ID> {
        const ID: u8 = ID;
    }
}
use report_id::*;

type ReportHidKeyboardReportId = ReportId<5>;

// I'm not currently using usbd-hid capabilities for defining a HID report
// descriptor because:
//   - I'm using my own custom types that are not compatible
// with it.
//   - I want to use zero-copy for byte interpretation, not really want to
// make the device to SerDe anything. There has to be (or I should make),
// something in between usbd-hid and just writing the bytes of the descriptor
// manually.
const REPORT_HID_KEYBOARD_DESCRIPTOR: [u8; 84] =[
    0x05, 0x0c,                                             // Usage Page (Consumer Devices)
    0x09, 0x01,                                             // Usage (Consumer Control)
    0xa1, 0x01,                                             // Collection (Application)
    0x85, 0x04,                                             //  Report ID (4)
    0x19, 0x01,                                             //  Usage Minimum (1)
    0x2a, 0xa0, 0x02,                                       //  Usage Maximum (672)
    0x15, 0x01,                                             //  Logical Minimum (1)
    0x26, 0xa0, 0x02,                                       //  Logical Maximum (672)
    0x95, 0x01,                                             //  Report Count (1)
    0x75, 0x10,                                             //  Report Size (16)
    0x81, 0x00,                                             //  Input (Data,Arr,Abs)
    0xc0,                                                   // End Collection
    0x05, 0x01,                                             // Usage Page (Generic Desktop)
    0x09, 0x06,                                             // Usage (Keyboard)
    0xa1, 0x01,                                             // Collection (Application)
    0x85, ReportHidKeyboardReportId::ID,                    //  Report ID (5)
    0x05, 0x07,                                             //  Usage Page (Keyboard)
    0x19, 0xe0,                                             //  Usage Minimum (224)
    0x29, 0xe7,                                             //  Usage Maximum (231)
    0x15, 0x00,                                             //  Logical Minimum (0)
    0x25, 0x01,                                             //  Logical Maximum (1)
    0x95, 0x08,                                             //  Report Count (8)
    0x75, 0x01,                                             //  Report Size (1)
    0x81, 0x02,                                             //  Input (Data,Var,Abs)
    0x05, 0x07,                                             //  Usage Page (Keyboard)
    0x19, REPORT_HID_USAGE_MIN as u8,                       //  Usage Minimum (REPORT_HID_USAGE_MIN)
    0x29, REPORT_HID_USAGE_MAX as u8,                       //  Usage Maximum (REPORT_HID_USAGE_MAX)
    0x15, 0x00,                                             //  Logical Minimum (0)
    0x25, 0x01,                                             //  Logical Maximum (1)
    0x95, REPORT_HID_USAGE_COUNT as u8,                     //  Report Count (REPORT_HID_USAGE_COUNT (must be aligned to 8 bits))
    0x75, 0x01,                                             //  Report Size (1)
    0x81, 0x02,                                             //  Input (Data,Var,Abs)
    0x05, 0x08,                                             //  Usage Page (LEDs)
    0x19, 0x01,                                             //  Usage Minimum (1)
    0x29, 0x05,                                             //  Usage Maximum (5)
    0x95, 0x05,                                             //  Report Count (5)
    0x75, 0x01,                                             //  Report Size (1)
    0x91, 0x02,                                             //  Output (Data,Var,Abs)
    0x95, 0x01,                                             //  Report Count (1)
    0x75, 0x03,                                             //  Report Size (3)
    0x91, 0x01,                                             //  Output (Cnst,Arr,Abs)
    0xc0,                                                   // End Collection
];

#[derive(IntoBytes, Immutable)]
#[repr(packed)]
pub struct ReportHidKeyboardOutReport {
    report_id: ReportHidKeyboardReportId,
    modifiers: u8,
    keys: ReportHidKeyboardUsageBitArray,
}
const _: () = assert!(size_of::<ReportHidKeyboardOutReport>() < USB_HID_READ_LEN, "Size for struct ReportHidKeyboardReport cannot be greater than 64 bytes.");

#[derive(IntoBytes, Immutable)]
#[repr(packed)]
pub struct ReportHidKeyboardContent {
    modifiers: u8,
    keys: ReportHidKeyboardUsageBitArray,
    leds: u8, // TODO LEDS are not send in the IN report, cause they're OUT (pun intended).
}

unsafe impl FromBytesSized for ReportHidKeyboardContent {}

impl ReportHidKeyboardContent {
    pub fn new() -> Self {
        ReportHidKeyboardContent { modifiers: 0, leds: 0, keys: BitArray::new() }
    }
}

impl ReportHidKeyboardOutReport {
    pub fn new() -> Self {
        ReportHidKeyboardOutReport { report_id: ReportHidKeyboardReportId::new(), content: ReportHidKeyboardContent::new() }
    }
}

pub struct ReportHidKeyboard<'a, B: UsbBus> {
    usb_dev: UsbDevice<'a, B>,
    ep: HIDClass<'a, B>,
    keyboard_report: ReportHidKeyboardOutReport,
    keyboard_report_dirty: bool
}

impl<'a, B: UsbBus> ReportHidKeyboard<'a, B> {
    pub fn alloc<'s>(allocator: &'a UsbBusAllocator<B>, settings: &'s BasicKeyboardSettings<'s, 'a>) -> Self {
        let mut hid_settings = HidClassSettings::default();
        hid_settings.protocol = HidProtocol::Keyboard;
        hid_settings.subclass = HidSubClass::NoSubClass;

        let ep = HIDClass::new_ep_in_with_settings(allocator, &REPORT_HID_KEYBOARD_DESCRIPTOR, settings.poll_ms, hid_settings);
        let usb_dev =
            UsbDeviceBuilder::new(allocator, UsbVidPid(settings.vid_pid.0, settings.vid_pid.1))
                .strings(settings.string_descriptors)
                .unwrap()
                .build();

        Self { usb_dev, ep, keyboard_report: ReportHidKeyboardOutReport::new(), keyboard_report_dirty: false }
    }

    fn ensure_keyboard_usage_within_bounds(key: KeyboardUsage) -> Result<(), KeyboardKeyChangeError> {
        if (key as u8) < (REPORT_HID_USAGE_MIN as u8) || (key as u8) > (REPORT_HID_USAGE_COUNT as u8) {
            Err(KeyboardKeyChangeError::Unsupported)
        } else {
            Ok(())
        }
    }
}

impl <'a, B: UsbBus> HidKeyboard for ReportHidKeyboard<'a, B> {
    #[inline]
    fn press_key(&mut self, key: KeyboardUsage) -> Result<(), KeyboardKeyChangeError> {
        Self::ensure_keyboard_usage_within_bounds(key)?;
        dev_info!("Pressing key: {} ({}) ({})", key as usize, REPORT_HID_USAGE_MIN as usize, key as usize - REPORT_HID_USAGE_MIN as usize);
        if self.keyboard_report.content.keys.set(key as usize - REPORT_HID_USAGE_MIN as usize) {
            self.keyboard_report_dirty = true;
            Ok(())
        } else {
            Err(KeyboardKeyChangeError::InvalidState)
        }
    }

    #[inline]
    fn release_key(&mut self, key: KeyboardUsage) -> Result<(), KeyboardKeyChangeError> {
        Self::ensure_keyboard_usage_within_bounds(key)?;
        if self.keyboard_report.content.keys.clear(key as usize - REPORT_HID_USAGE_MIN as usize) {
            self.keyboard_report_dirty = true;
            Ok(())
        } else {
            Err(KeyboardKeyChangeError::InvalidState)
        }
    }

    fn press_media_key(&mut self, key: MediaKey) -> Result<(), KeyboardKeyChangeError> {
        todo!()
    }

    fn release_media_key(&mut self, key: MediaKey) -> Result<(), KeyboardKeyChangeError> {
        todo!()
    }

    fn poll(&mut self) -> Result<bool, KeyboardPollError> {
        if self.keyboard_report_dirty {
            // Technically, we shouldn't push output data (like LEDs data), but
            // since the host is likely gonna ignore it, we can just be cheap on
            // it?
            let out = [5, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,0,0b10000001];
            dev_info!("Usb tx start. Len: {}", out.len());
            match self.ep.push_raw_input(&out) {
                Ok(_) => {
                    self.keyboard_report_dirty = false;
                    dev_info!("Usb tx ok");
                },
                Err(UsbError::WouldBlock) => {
                    // TODO NOT DISCARD THIS REPORT. JUST FOR DEBUG.
                    self.keyboard_report_dirty = false;
                },
                Err(e) => {
                    return Err(e.into())
                }
            }
        }

        if self.usb_dev.poll(&mut [&mut self.ep]) {
            let mut buf: [u8; 64] = [0u8; 64];
            let report_info =
                match self.ep.pull_raw_report(&mut buf) {
                    Ok(r) => r,
                    Err(UsbError::WouldBlock) => {
                        return Ok(false)
                    },
                    Err(e) => {
                        return Err(e.into());
                    }
                };

            dev_info!("Received report: {:?} {}", report_info.report_type, report_info.report_id);

            match report_info.report_id {
                ReportHidKeyboardReportId::ID => {
                    let content = ReportHidKeyboardContent::ref_from_byte_array(&buf);
                    self.keyboard_report.content.leds = content.leds;
                    dev_info!("LEDs: {:b}", content.leds);
                }
                report_id => {
                    return Err(KeyboardPollError::UnknownReport(report_id));
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

// TODO Not yet implemented
struct BootHidKeyboard {}

struct ReportBootHidKeyboard {}

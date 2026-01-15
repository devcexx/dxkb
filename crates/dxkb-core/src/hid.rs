use core::fmt::{Display, Write};

use bitflags::bitflags;
use dxkb_common::{
    dev_error, dev_info, util::{BitArray, ConstU8, ConstU8Like, FromByteArray, FromBytesSized}
};
use usb_device::{
    bus::{UsbBus, UsbBusAllocator},
    device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid},
};
use usbd_hid::{
    UsbError,
    descriptor::{
        KeyboardReport, KeyboardUsage, MediaKey, SerializedDescriptor, gen_hid_descriptor,
    },
    hid_class::{
        HIDClass, HidClassSettings, HidCountryCode, HidProtocol, HidSubClass, ProtocolModeConfig,
    },
};
use zerocopy::{Immutable, IntoBytes};

enum LookOrFindEmptyMutResult<'a, A> {
    Found(&'a mut A),
    Empty(&'a mut A),
    Full
}

fn lookup_or_find_empty_mut<'a, A>(haystick: &'a mut [A], needle: &'a A, empty: &'a A) -> LookOrFindEmptyMutResult<'a, A> where A: Eq {
    let mut last_empty_idx: isize = -1;
    for i in 0..haystick.len() {
        if &haystick[i] == needle {
            return LookOrFindEmptyMutResult::Found(&mut haystick[i]);
        } else if &haystick[i] == empty {
            last_empty_idx = i as isize;
        }
    }

    if last_empty_idx == -1 {
        LookOrFindEmptyMutResult::Full
    } else {
        LookOrFindEmptyMutResult::Empty(&mut haystick[last_empty_idx as usize])
    }
}

pub enum KeyboardKeyChangeError {
    Unsupported,
    InvalidState,
    Rollover
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
    MalformedOutReport,
}

impl Display for KeyboardPollError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            KeyboardPollError::UsbError(usb_error) => {
                write!(f, "Usb error: {:?}", usb_error)
            },
            KeyboardPollError::UnknownReport(id) => {
                write!(f, "Unknown report recv from host: {:?}", id)
            },
            KeyboardPollError::MalformedOutReport => {
                write!(f, "Malformed report")
            },
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

    fn press_consumer_control_key(&mut self, key: u16) -> Result<(), KeyboardKeyChangeError>;
    fn release_consumer_control_key(&mut self, key: u16) -> Result<(), KeyboardKeyChangeError>;

    fn poll(&mut self) -> Result<bool, KeyboardPollError>;

    // TODO leds
}

// The linux kernel recognizes ~ 624 consumer control keys. (ref:
// https://github.com/torvalds/linux/blob/e0d4140e804380ae898da1e4c58c21e6323415a4/drivers/hid/hid-input.c#L1089).
// Since we're already in the > 8 bit territory, and it is probably not worth it
// at this point to start playing with data that is not byte-aligned, just
// extending the min/max values to comprend the whole CC specification.
const REPORT_HID_CC_USAGE_MIN: u16 = 0x01;
const REPORT_HID_CC_USAGE_MAX: u16 = 0x514;
const REPORT_HID_CC_USAGE_COUNT: usize = (REPORT_HID_CC_USAGE_MAX as usize) - (REPORT_HID_CC_USAGE_MIN as usize) + 1;


const REPORT_HID_KB_USAGE_MIN: KeyboardUsage = KeyboardUsage::KeyboardErrorRollOver;
const REPORT_HID_KB_USAGE_MAX: KeyboardUsage = KeyboardUsage::Reserved;
const REPORT_HID_KB_USAGE_COUNT: usize =
    (REPORT_HID_KB_USAGE_MAX as usize) - (REPORT_HID_KB_USAGE_MIN as usize) + 1;

// Always try to read the maximum packet size allowed in USB 2 spec. usbd-hid
// crate hardcodes 64 as maximum packet size for USB endpoint, and the
// synopsys-usb-otg won't let you to read or write more than a data packet per transaction so... yeah, 64.
const USB_HID_READ_LEN: usize = 64;

const _: () = assert!(
    REPORT_HID_KB_USAGE_COUNT % 8 == 0,
    "Report protocol HID keyboard keys usage counts that are not divisible by 8 are not supported at this point"
);

type ReportHidKeyboardUsageBitArray = BitArray<REPORT_HID_KB_USAGE_COUNT>;
type ReportHidKeyboardReportId = ConstU8<5>;
type ReportHidConsumerControlReportId = ConstU8<4>;

const fn u16_lobits(n: u16) -> u8 {
    (n & 0xff) as u8
}

const fn u16_hibits(n: u16) -> u8 {
    (n >> 8) as u8
}

// I'm not currently using usbd-hid capabilities for defining a HID report
// descriptor because:
//   - I'm using my own custom types that are not compatible
// with it.
//   - I want to use zero-copy for byte interpretation, not really want to
// make the device to SerDe anything. There has to be (or I should make),
// something in between usbd-hid and just writing the bytes of the descriptor
// manually.
const REPORT_HID_KEYBOARD_DESCRIPTOR: [u8; 76] =[
    0x05, 0x0c,                                             // Usage Page (Consumer Devices)
    0x09, 0x01,                                             // Usage (Consumer Control)
    0xa1, 0x01,                                             // Collection (Application)
    0x85, 0x04,                                             //  Report ID (4)
    0x95, 0x01,                                             //  Report Count (1)
    0x75, 0x08,                                             //  Report Size (8)
    0x81, 0x01,                                             //  Output (Cnst,Arr,Abs) Convenience padding to align the pressed CC keys to 16-bit word.
    0x1a, u16_lobits(REPORT_HID_CC_USAGE_MIN), u16_hibits(REPORT_HID_CC_USAGE_MIN), //  Usage Minimum (FIXME using a 2-byte tag size so we can always encode the size in two bytes, although it is not necessary for HID)
    0x2a, u16_lobits(REPORT_HID_CC_USAGE_MAX), u16_hibits(REPORT_HID_CC_USAGE_MAX), //  Usage Maximum
    0x16, u16_lobits(REPORT_HID_CC_USAGE_MIN), u16_hibits(REPORT_HID_CC_USAGE_MIN), //  Logical Minimum
    0x26, u16_lobits(REPORT_HID_CC_USAGE_MAX), u16_hibits(REPORT_HID_CC_USAGE_MAX), //  Logical Maximum
    0x95, 0x1f,                                             //  Report Count (31) 31 reports * 16 bits each = 62 bytes < 64 bytes, leaving space for 1 byte for the report ID.
    0x75, 0x10,                                             //  Report Size (16)
    0x81, 0x00,                                             //  Input (Data,Arr,Abs)
    0xc0,                                                   // End Collection
    0x05, 0x01,                                             // Usage Page (Generic Desktop)
    0x09, 0x06,                                             // Usage (Keyboard)
    0xa1, 0x01,                                             // Collection (Application)
    0x85, ReportHidKeyboardReportId::N,                     //  Report ID (5)
    0x05, 0x07,                                             //  Usage Page (Keyboard)
    0x19, REPORT_HID_KB_USAGE_MIN as u8,                    //  Usage Minimum (REPORT_HID_USAGE_MIN)
    0x29, REPORT_HID_KB_USAGE_MAX as u8,                    //  Usage Maximum (REPORT_HID_USAGE_MAX)
    0x15, 0x00,                                             //  Logical Minimum (0)
    0x25, 0x01,                                             //  Logical Maximum (1)
    0x95, REPORT_HID_KB_USAGE_COUNT as u8,                  //  Report Count (REPORT_HID_USAGE_COUNT (must be aligned to 8 bits))
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
pub struct ReportHidKeyboardInReport {
    report_id: ReportHidKeyboardReportId,
    keys: ReportHidKeyboardUsageBitArray,
}
const _: () = assert!(
    size_of::<ReportHidKeyboardInReport>() < USB_HID_READ_LEN,
    "Size for struct ReportHidKeyboardReport cannot be greater than 64 bytes."
);

impl ReportHidKeyboardInReport {
    pub const fn new() -> Self {
        ReportHidKeyboardInReport {
            report_id: ReportHidKeyboardReportId::I,
            keys: ReportHidKeyboardUsageBitArray::new()
        }
    }
}

#[derive(IntoBytes, Immutable)]
#[repr(C, packed(2))]
pub struct ReportHidConsumerControlInReport {
    report_id: ReportHidConsumerControlReportId,
    _pad1: ConstU8<0>, // Explicit padding to keep the buttons aligned to 2 bytes. Included in the report descriptor.
    pressed_buttons: [u16; 31]
}

impl ReportHidConsumerControlInReport {
    pub const fn new() -> Self {
        Self { report_id: ReportHidConsumerControlReportId::I, _pad1: ConstU8::I, pressed_buttons: [0u16; 31] }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Leds: u8 {
        const NUM_LOCK    = 0b00000001;
        const CAPS_LOCK   = 0b00000010;
        const SCROLL_LOCK = 0b00000100;
        const COMPOSE     = 0b00001000;
        const KANA        = 0b00010000;
    }
}

struct MutableReport<R> {
    pub report: R,
    dirty: bool,
}

impl<R> MutableReport<R> {
    fn new(report: R) -> MutableReport<R> {
        Self {
            report,
            dirty: false
        }
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn set_dirty(&mut self) {
        self.dirty = true;
    }

    fn clear_dirty(&mut self) {
        self.dirty = false;
    }
}

pub struct ReportHidKeyboard<'a, B: UsbBus> {
    usb_dev: UsbDevice<'a, B>,
    ep: HIDClass<'a, B>,
    kb: MutableReport<ReportHidKeyboardInReport>,
    cc: MutableReport<ReportHidConsumerControlInReport>,
    leds: Leds,
}

impl<'a, B: UsbBus> ReportHidKeyboard<'a, B> {
    pub fn alloc<'s>(
        allocator: &'a UsbBusAllocator<B>,
        settings: &'s BasicKeyboardSettings<'s, 'a>,
    ) -> Self {
        let mut hid_settings = HidClassSettings::default();
        hid_settings.protocol = HidProtocol::Keyboard;
        hid_settings.subclass = HidSubClass::NoSubClass;

        let ep = HIDClass::new_ep_in_with_settings(
            allocator,
            &REPORT_HID_KEYBOARD_DESCRIPTOR,
            settings.poll_ms,
            hid_settings,
        );
        let usb_dev =
            UsbDeviceBuilder::new(allocator, UsbVidPid(settings.vid_pid.0, settings.vid_pid.1))
                .strings(settings.string_descriptors)
                .unwrap()
                .build();

        Self {
            usb_dev,
            ep,
            kb: MutableReport::new(ReportHidKeyboardInReport::new()),
            cc: MutableReport::new(ReportHidConsumerControlInReport::new()),
            leds: Leds::empty(),
        }
    }

    fn ensure_keyboard_usage_within_bounds(
        key: KeyboardUsage,
    ) -> Result<(), KeyboardKeyChangeError> {
        if (key as u8) < (REPORT_HID_KB_USAGE_MIN as u8)
            || (key as u8) > (REPORT_HID_KB_USAGE_MAX as u8)
        {
            Err(KeyboardKeyChangeError::Unsupported)
        } else {
            Ok(())
        }
    }

    fn ensure_cc_within_bounds(
        cc_btn: u16,
    ) -> Result<(), KeyboardKeyChangeError> {
        if cc_btn < (REPORT_HID_CC_USAGE_MIN as u16)
            || cc_btn > (REPORT_HID_CC_USAGE_MAX as u16)
        {
            Err(KeyboardKeyChangeError::Unsupported)
        } else {
            Ok(())
        }
    }

    fn do_rx(&mut self) -> Result<bool, KeyboardPollError> {
        if self.usb_dev.poll(&mut [&mut self.ep]) {
            let mut buf: [u8; 64] = [0u8; 64];
            let report_info = match self.ep.pull_raw_report(&mut buf) {
                Ok(r) => r,
                Err(UsbError::WouldBlock) => return Ok(false),
                Err(e) => {
                    return Err(e.into());
                }
            };

            dev_info!(
                "Received report: {:?} {}",
                report_info.report_type,
                report_info.report_id
            );

            match report_info.report_id {
                ReportHidKeyboardReportId::N => {
                    // I'm not gonna even bother to create a struct to read a single byte, at least for now.
                    if report_info.len < 2 {
                        dev_error!("Received not enough bytes for OUT Report");
                        return Err(KeyboardPollError::MalformedOutReport)
                    }
                    let leds = Leds::from_bits_retain(buf[1]);
                    dev_info!("LEDs: {:?} ({:b})", leds, buf[1]);
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

    fn do_tx_report<R: IntoBytes + Immutable>(ep: &mut HIDClass<'a, B>, report: &mut MutableReport<R>) -> Result<(), KeyboardPollError> {
        if report.is_dirty() {
            dev_info!("Send report: {:x?}", &report.report.as_bytes());
            match ep.push_raw_input(&report.report.as_bytes()) {
                Ok(_) => {
                    report.clear_dirty();
                    Ok(())
                }
                Err(UsbError::WouldBlock) => Ok(()),
                Err(e) => return Err(e.into()),
            }
        } else {
            Ok(())
        }
    }
}

impl<'a, B: UsbBus> HidKeyboard for ReportHidKeyboard<'a, B> {
    #[inline]
    fn press_key(&mut self, key: KeyboardUsage) -> Result<(), KeyboardKeyChangeError> {
        Self::ensure_keyboard_usage_within_bounds(key)?;
        dev_info!(
            "Pressing key: {} ({}) ({})",
            key as usize,
            REPORT_HID_KB_USAGE_MIN as usize,
            key as usize - REPORT_HID_KB_USAGE_MIN as usize
        );
        if self
            .kb
            .report
            .keys
            .set(key as usize - REPORT_HID_KB_USAGE_MIN as usize)
        {
            self.kb.set_dirty();
            Ok(())
        } else {
            Err(KeyboardKeyChangeError::InvalidState)
        }
    }

    #[inline]
    fn release_key(&mut self, key: KeyboardUsage) -> Result<(), KeyboardKeyChangeError> {
        Self::ensure_keyboard_usage_within_bounds(key)?;
        if self
            .kb
            .report
            .keys
            .clear(key as usize - REPORT_HID_KB_USAGE_MIN as usize)
        {
            self.kb.set_dirty();
            Ok(())
        } else {
            Err(KeyboardKeyChangeError::InvalidState)
        }
    }

    fn press_consumer_control_key(&mut self, key: u16) -> Result<(), KeyboardKeyChangeError> {
        dev_info!("Press: {:x}", key);
        Self::ensure_cc_within_bounds(key)?;
        dev_info!("Press2: {:x}", key);
        match lookup_or_find_empty_mut(&mut self.cc.report.pressed_buttons, &key, &0) {
            LookOrFindEmptyMutResult::Found(_) => {
                // Already pressed
                dev_info!("Already pressed: {:x}", key);
                Err(KeyboardKeyChangeError::InvalidState)
            },
            LookOrFindEmptyMutResult::Empty(empty) => {
                // Not pressed, but an empty space found
                dev_info!("Saved: {:x}", key);
                *empty = key;
                self.cc.set_dirty();
                Ok(())
            },
            LookOrFindEmptyMutResult::Full => {
                // Kaboom
                dev_info!("Full: {:x}", key);
                Err(KeyboardKeyChangeError::Rollover)
            },
        }
    }

    fn release_consumer_control_key(&mut self, key: u16) -> Result<(), KeyboardKeyChangeError> {
        Self::ensure_cc_within_bounds(key)?;
        match lookup_or_find_empty_mut(&mut self.cc.report.pressed_buttons, &key, &0) {
            LookOrFindEmptyMutResult::Found(pressed) => {
                // Pressed, need to unpress
                *pressed = 0;
                self.cc.set_dirty();
                Ok(())
            },
            LookOrFindEmptyMutResult::Empty(_) | LookOrFindEmptyMutResult::Full => {
                // Wasn't pressed
                Err(KeyboardKeyChangeError::InvalidState)
            },
        }
    }

    fn poll(&mut self) -> Result<bool, KeyboardPollError> {
        Self::do_tx_report(&mut self.ep, &mut self.kb)?;
        Self::do_tx_report(&mut self.ep, &mut self.cc)?;
        self.do_rx()
    }
}

// TODO Not yet implemented
struct BootHidKeyboard {}

struct ReportBootHidKeyboard {}

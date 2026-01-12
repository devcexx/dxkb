#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr_concat)]
#![feature(concat_idents)]
#![no_std]

use usbd_hid::descriptor::{KeyboardUsage, MediaKey};

pub mod hid;
pub mod keyboard;
pub mod keys;

struct BootReport {
    keys: [u8; 6],
}

struct ExtendedReport {
    media_keys: [u8; 6],
}

struct BootWithExtendedReport {}

trait KeyboardUsbReporter<B> {
    fn add_keyboard_key(k: KeyboardUsage);
    fn rm_keyboard_key(k: KeyboardUsage);
    fn add_media_key(k: MediaKey);
    fn rm_media_key(k: MediaKey);
}

struct ReportMan {}

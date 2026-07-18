use core::{panic::PanicInfo, sync::atomic::{self, Ordering}, time::Duration};
use core::cell::RefCell;
use cortex_m::{interrupt::{self, Mutex, free}, peripheral::ITM};
use cortex_m::iprintln;
use dxkb_common::{dev_error, time::Clock};
use dxkb_peripheral::clock::DWTClock;

static CONFIG: Mutex<RefCell<Option<UsbItmPanicHandlerConfig>>> = Mutex::new(RefCell::new(Option::None));
const PANIC_HEADER: &str = "!!!!PANICPANICPANICPANICPANICPANIC!!!!";

pub struct UsbItmPanicHandlerConfig {
    pub usb_reset: &'static (dyn Fn() + Send + Sync),
    pub usb_debug_poll: &'static (dyn Fn() + Send + Sync),
    pub clk: DWTClock
}

pub fn setup(config: UsbItmPanicHandlerConfig) {
    free(|cs| {
        *CONFIG.borrow(cs).borrow_mut() = Some(config);
    });
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let itm = unsafe { &mut *ITM::ptr() };
    let stim = &mut itm.stim[0];

    iprintln!(stim, "{}", PANIC_HEADER);
    iprintln!(stim, "{}", info);

    let config = free(|cs| {
        CONFIG.borrow(cs).borrow_mut().take()
    });

    // Yes, I'm doing a free and then a interrupt::free, just for avoiding using an unsafe
    interrupt::disable();

    if let Some(config) = config {
        (config.usb_reset)();
        dev_error!("{}", PANIC_HEADER);
        dev_error!("{}", info);
        let mut now = config.clk.current_instant();
        loop {
            (config.usb_debug_poll)();
            if config.clk.elapsed_since(now) > Duration::from_secs(1) {
                dev_error!("{}", PANIC_HEADER);
                dev_error!("{}", info);
                now = config.clk.current_instant();
            }
        }
    } else {
        iprintln!(stim, "Note: The USB ITM panic handler is being in use, but it hasn't been configured properly!");
        loop {
            // Copied from panic_itm
            // add some side effect to prevent this from turning into a UDF instruction
            // see rust-lang/rust#28728 for details
            atomic::compiler_fence(Ordering::SeqCst);
        }
    }
}

use core::hint::black_box;

use stm32f4xx_hal::{rcc::Clocks, time::Hertz};
use vcell::VolatileCell;
use cortex_m::peripheral::DWT;

pub struct CycleCounter {
    last_cycnt_read: u32,

    // This below won't work in those cases where the uptime jiffies
    // are queried just right after an overflow. So this is not a good idea I guess.


    // Since the CYCNT register is only 32 bits, if the core clock is
    // 96 MHz, it will overflow in ~45 secs. For avoiding that, I will
    // compute the total uptime in a 64 bit number, in which the LSB
    // are just the value of the CYCNT register; and the MSB will be
    // stored here, and will increment every time it is detected that
    // the CYCNT has overflow.
    uptime_jiffies_hibits: VolatileCell<u32>,
    hclk: Hertz
}

// impl CycleCounter {
//     pub fn new(mut dwt: DWT, clocks: &Clocks) -> Self {
//         // For now I don't need the DWT, I just need it to prove that
//         // no one can disable the cycle counting outside of this instance.
//         dwt.enable_cycle_counter();
//         let cur = DWT::cycle_count();
//         Self {
//             last_jiffies: cur as u64,
//             uptime_jiffies: VolatileCell::new(cur as u64),
//             hclk: clocks.hclk()
//         }
//     }

//     /// Refreshes the current cycle counter. It needs to be executed
//     /// at least once every `u32::MAX` jiffies.
//     #[inline(never)]
//     pub fn refresh(&mut self) {
//         //let cur = DWT::cycle_count();
//         self.uptime_jiffies.set(black_box(12));

// //        self.uptime_jiffies.set(self.uptime_jiffies.get() + cur - self.last_jiffies);
//     }

//     pub fn uptime_jiffies(&mut self) -> u64 {
//         self.uptime_jiffies.get() + DWT::cycle_count() as u64 - self.last_jiffies
//     }
// }

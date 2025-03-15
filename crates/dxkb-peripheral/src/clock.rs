use core::time::Duration;

use dxkb_common::{dev_info, time::{Clock, TimeDiff}};
use enumflags2::BitFlags;
use stm32f4xx_hal::{pac::{DCB, DWT}, rcc::Clocks, timer::{Event, Instance}};
use vcell::VolatileCell;

pub struct TIMClock<TIM: Instance> {
    uptime_hibits: VolatileCell<u32>,
    timer: TIM,
}

impl<TIM: Instance<Width = u32>> TIMClock<TIM> {
    fn gcd(mut a: u64, mut b: u64) -> u64 {
        while b != 0 {
            let temp = b;
            b = a % b;
            a = temp;
        }
        a
    }

    pub fn init(mut tim: TIM, clocks: &Clocks) -> Self {
        let tim_clk = TIM::timer_clock(clocks).raw() as u64;
        // TODO Check arch/arm/mach-stm32/timer.c from linux-emcraft.
        let max_tim_freq = Self::gcd(1_000_000_000, tim_clk);
        let tim_psc = tim_clk / max_tim_freq;
        let nanos_per_tick = 1_000_000_000 / max_tim_freq;

        unsafe {
            TIM::enable_unchecked();
            TIM::reset_unchecked();
        }

        tim.set_prescaler(u16::try_from(tim_psc - 1).unwrap());
        unsafe {
            // SAFETY: Timer resolution is 32 bits as per TIM type constraints.
            tim.set_auto_reload_unchecked(0x1999999); // TODO Use a correct value
        }

        tim.listen_event(Some(BitFlags::ALL), Some(Event::Update.into()));
        tim.clear_interrupt_flag(BitFlags::ALL);
        tim.enable_counter(true);
        tim.trigger_update();

        dev_info!("TIM Clock started with a resolution of {} nanos per tick. Prescaler: {}", nanos_per_tick, tim_psc);

        Self {
            uptime_hibits: VolatileCell::new(0),
            timer: tim,
        }
    }


    pub fn get_time(&self) -> u64 {
        // It is hard
        todo!()
    }

    #[inline(always)]
    pub fn handle_intr(&mut self) {
        todo!()

    }
}


#[derive(Clone)]
pub struct DWTClock {
    clock_freq: u32
}

#[derive(Clone, Copy)]
pub struct DWTInstant {
    cycles: u32,
}

impl DWTClock {
    pub fn new(clocks: &Clocks, dcb: &mut DCB, dwt: &mut DWT) -> Self {
        dcb.enable_trace();
        dwt.enable_cycle_counter();

        Self {
            clock_freq: clocks.sysclk().raw()
        }
    }

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

use core::time::Duration;

#[derive(Clone, Copy, Debug, Default)]
pub struct Instant {
    nanos: u64
}

impl Instant {
    pub const fn new(nanos: u64) -> Self {
        Instant {
            nanos
        }
    }

    // TODO These implementations are *wrong* *as fuck* because they
    // do not take into consideration the maxixum number of bits that
    // the nanos will have, to calculate the wrapping. In STM32, this
    // resolution goes up to 32 bits. This must be moved to the Clock
    // trait for ensuring that wrapping is handled properly.
    pub fn elapsed_nanos<C: Clock>(self, clock: &C) -> u64 {
        clock.current_nanos() - self.nanos
    }

    pub fn elapsed<C: Clock>(self, clock: &C) -> Duration {
        Duration::from_nanos(clock.current_nanos() - self.nanos)
    }

    pub fn nanos(self) -> u64 {
        self.nanos
    }
}

pub enum TimeDirection {
    Forward(Duration),
    Backward(Duration)
}

pub trait Clock {
    fn current_cycle(&self) -> u32;
    fn current_nanos(&self) -> u64;
    fn current_instant(&self) -> Instant {
        Instant { nanos: self.current_nanos() }
    }

    /// Reliably calculates the direction of time between two
    /// instants, taking into account limitations of the current
    /// clock. (E.g when the clock resets and when back to zero
    /// because of the underlying implementation).
    fn time_direction(possible_newer: Instant, possible_older: Instant) -> TimeDirection {
        // TODO!
        unimplemented!()
    }
}

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

    pub fn elapsed_nanos<C: Clock>(self, clock: &C) -> u64 {
        clock.current_nanos() - self.nanos
    }

    pub fn elapsed<C: Clock>(self, clock: &C) -> Duration {
        Duration::from_nanos(clock.current_nanos() - self.nanos)
    }
}

pub trait Clock {
    fn current_cycle(&self) -> u32;
    fn current_nanos(&self) -> u64;
    fn current_instant(&self) -> Instant {
        Instant { nanos: self.current_nanos() }
    }
}

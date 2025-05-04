use core::time::Duration;

pub enum TimeDiff {
    Forward(Duration),
    Backward(Duration),
}

impl TimeDiff {
    fn ensure_forward(self) -> Duration {
        match self {
            TimeDiff::Forward(duration) => duration,
            TimeDiff::Backward(_) => panic!("Time went backwards!"),
        }
    }
}

// TODO I think that having a clock with a limited range of u32 is not ideal for some scenarios:
// - For things like measuring short period times, is great because it is fast af.
// - For storing the complete time, we might need to do something like disabling interrupts, frequently update the clock, etc, which is less performant. I guess this option should only be used for things that actually need it.

pub trait Clock {
    type TInstant: Copy;

    fn current_instant(&self) -> Self::TInstant;

    fn elapsed_since(&self, past_instant: Self::TInstant) -> Duration {
        self.diff(self.current_instant(), past_instant)
            .ensure_forward()
    }

    fn diff(&self, newer: Self::TInstant, older: Self::TInstant) -> TimeDiff;
    fn nanos(&self, instant: Self::TInstant) -> u64;
}

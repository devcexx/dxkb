pub trait Clock {
    fn current_cycle(&self) -> u32;
    fn current_nanos(&self) -> u64;
}

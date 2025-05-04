#[derive(Debug)]
pub enum BusPollError {
    WouldBlock,
    BufferOverflow,
}

#[derive(Debug)]
pub enum BusTransferError {
    WouldBlock,
}

pub trait BusWrite {
    fn transfer(&mut self, buf: &[u8]) -> Result<(), BusTransferError>;
    fn is_tx_busy(&self) -> bool;
}

pub trait BusRead {
    fn poll_next(&self, buf: &mut [u8]) -> Result<u16, BusPollError>;
}

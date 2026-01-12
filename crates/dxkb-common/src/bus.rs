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

pub struct NullBus;

impl BusWrite for NullBus {
    fn transfer(&mut self, buf: &[u8]) -> Result<(), BusTransferError> {
        Ok(())
    }

    fn is_tx_busy(&self) -> bool {
        false
    }
}

impl BusRead for NullBus {
    fn poll_next(&self, _buf: &mut [u8]) -> Result<u16, BusPollError> {
        Err(BusPollError::WouldBlock)
    }
}

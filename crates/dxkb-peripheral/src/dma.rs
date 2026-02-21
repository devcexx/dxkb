use stm32f4xx_hal::{dma::StreamX, pac::{Interrupt, DMA1, DMA2}};
use crate::InterruptReceiver;

macro_rules! dma_interrupt_impl {
    ($dma:ident, $stream:literal, $intr:ident) => {
        impl InterruptReceiver for StreamX<$dma, $stream> where {
            const INTERRUPT: Interrupt = Interrupt::$intr;
        }
    };
}

dma_interrupt_impl!(DMA1, 0, DMA1_STREAM0);
dma_interrupt_impl!(DMA1, 1, DMA1_STREAM1);
dma_interrupt_impl!(DMA1, 2, DMA1_STREAM2);
dma_interrupt_impl!(DMA1, 3, DMA1_STREAM3);
dma_interrupt_impl!(DMA1, 4, DMA1_STREAM4);
dma_interrupt_impl!(DMA1, 5, DMA1_STREAM5);
dma_interrupt_impl!(DMA1, 6, DMA1_STREAM6);
dma_interrupt_impl!(DMA1, 7, DMA1_STREAM7);

dma_interrupt_impl!(DMA2, 0, DMA2_STREAM0);
dma_interrupt_impl!(DMA2, 1, DMA2_STREAM1);
dma_interrupt_impl!(DMA2, 2, DMA2_STREAM2);
dma_interrupt_impl!(DMA2, 3, DMA2_STREAM3);
dma_interrupt_impl!(DMA2, 4, DMA2_STREAM4);
dma_interrupt_impl!(DMA2, 5, DMA2_STREAM5);
dma_interrupt_impl!(DMA2, 6, DMA2_STREAM6);
dma_interrupt_impl!(DMA2, 7, DMA2_STREAM7);

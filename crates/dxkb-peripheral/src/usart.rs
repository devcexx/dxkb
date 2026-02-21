use stm32f4xx_hal::pac::Interrupt;
use crate::InterruptReceiver;

macro_rules! usart_interrupt_impl {
    ($intr:ident) => {
        impl InterruptReceiver for stm32f4xx_hal::pac::$intr where {
            const INTERRUPT: Interrupt = Interrupt::$intr;
        }
    };
}

#[cfg(feature = "stm32f411")]
usart_interrupt_impl!(USART1);

#[cfg(feature = "stm32f411")]
usart_interrupt_impl!(USART2);

#[cfg(feature = "stm32f411")]
usart_interrupt_impl!(USART6);

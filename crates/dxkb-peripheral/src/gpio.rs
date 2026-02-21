use stm32f4xx_hal::{gpio::Pin, pac::Interrupt};
use crate::InterruptReceiver;

macro_rules! gpio_port_impl {
    ($($port:ident $portc:literal),*) => {
        $(
        impl GpioPort for stm32f4xx_hal::pac::$port {
            unsafe fn idr_value() -> u32 {
                unsafe { Self::steal().idr().read().bits() }
            }
        }

        impl GpioPort for GpioX<$portc> {
            unsafe fn idr_value() -> u32 {
                unsafe { stm32f4xx_hal::pac::$port::steal().idr().read().bits() }
            }
        }
        )*
    }
}

macro_rules! gpio_exti_interrupt_impl {
    ($pnum:literal, $intr:ident) => {
        impl<const P: char, MODE> InterruptReceiver for Pin<P, $pnum, MODE> where {
            const INTERRUPT: Interrupt = Interrupt::$intr;
        }
    };
}

pub struct GpioX<const PORT: char> {}

pub trait GpioPort {
    // SAFETY: Read values might be valid or not depending on the
    // input/output status of the pins in the ports. Caller must be
    // aware of the fact that returned value might not be valid.
    unsafe fn idr_value() -> u32;
}

#[cfg(feature = "stm32f411")]
gpio_port_impl!(GPIOA 'A', GPIOB 'B', GPIOC 'C', GPIOD 'D', GPIOE 'E', GPIOH 'H');

gpio_exti_interrupt_impl!(0, EXTI0);
gpio_exti_interrupt_impl!(1, EXTI1);
gpio_exti_interrupt_impl!(2, EXTI2);
gpio_exti_interrupt_impl!(3, EXTI3);
gpio_exti_interrupt_impl!(4, EXTI4);
gpio_exti_interrupt_impl!(5, EXTI9_5);
gpio_exti_interrupt_impl!(6, EXTI9_5);
gpio_exti_interrupt_impl!(7, EXTI9_5);
gpio_exti_interrupt_impl!(8, EXTI9_5);
gpio_exti_interrupt_impl!(9, EXTI9_5);
gpio_exti_interrupt_impl!(10, EXTI15_10);
gpio_exti_interrupt_impl!(11, EXTI15_10);
gpio_exti_interrupt_impl!(12, EXTI15_10);
gpio_exti_interrupt_impl!(13, EXTI15_10);
gpio_exti_interrupt_impl!(14, EXTI15_10);
gpio_exti_interrupt_impl!(15, EXTI15_10);

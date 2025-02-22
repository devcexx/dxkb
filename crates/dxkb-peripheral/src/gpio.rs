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

pub struct GpioX<const PORT: char> {}

pub trait GpioPort {
    // SAFETY: Read values might be valid or not depending on the
    // input/output status of the pins in the ports. Caller must be
    // aware of the fact that returned value might not be valid.
    unsafe fn idr_value() -> u32;
}

#[cfg(feature = "stm32f411")]
gpio_port_impl!(GPIOA 'A', GPIOB 'B', GPIOC 'C', GPIOD 'D', GPIOE 'E', GPIOH 'H');

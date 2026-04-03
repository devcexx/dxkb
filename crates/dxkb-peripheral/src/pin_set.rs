use core::marker::PhantomData;
use dxkb_common::util::{ConstCond, IsTrue};
use stm32f4xx_hal::gpio::{DynamicPin, Speed};
use stm32f4xx_hal::pac::gpioa;
use stm32f4xx_hal::pac::gpioa::otyper::OT0;
use stm32f4xx_hal::pac::gpioa::moder::MODER0;
use stm32f4xx_hal::pac::gpioa::pupdr::PUPDR0;

/**
 * Represent a set of pins whose configuration and state can be manipulated
 * efficiently together. The configuration of the pins and setting its values
 * happen efficiently by pre-computing in compile time the values of the
 * registers that should be appended to the runtime registers values, based on
 * the pins that conform the set. Although the implementors of this trait must
 * support building sets of pins that are located in different physical ports,
 * since the STM32F411 provides per-port GPIO, the lowest number of operations
 * for configuring the pins will happen when every pin in the set is part of the
 * same GPIO port.
 *
 * For example, if a pin set is composed by the pins PA11, PA12, PB1 and PC1,
 * and [`make_input_pull_up`](PinSet::make_input_pull_up) is called, then three
 * different mode setting operations will ran, one for each port that composes
 * the set, regardless of the number of pins in each port.
 */
 #[diagnostic::on_unimplemented(asdf)]
pub trait PinSet {
    const NUM_PINS: usize;
    const NUM_PORTS: usize;
    const PIN_REFS: &'static [(char, u8)];

    fn make_input_pull_up(&mut self);
    fn make_input_pull_down(&mut self);
    fn make_input_floating(&mut self);
    fn make_output_push_pull(&mut self);
    fn make_output_open_drain(&mut self);
    fn read(&self) -> PinSetRead<Self> where Self: Sized;
    fn write_all(&mut self, new_value: bool);
    fn write_single(&mut self, index: u32, new_value: bool);
    fn set_speed(&mut self, speed: Speed);
}

 #[diagnostic::on_unimplemented(asdf)]
pub trait PinSetSized<const NUM_PINS: usize>: PinSet {

}

struct OwnedSlice<T, const N: usize> {
    buf: [T; N],
    len: usize,
}

impl<T, const N: usize> OwnedSlice<T, N> {
    pub const fn new(buf: [T; N], len: usize) -> Self {
        Self { buf, len }
    }

    pub const fn get_ref(&self) -> &[T] {
        &self.buf[0..self.len]
    }
}

/**
 * Computes a 32-bit number whose value is equal to the given value (truncated
 * to bit_width bits) placed at the given indexes.
 *
 * For example, if indexes = [0, 2], value = 0b11 and bit_width = 2, then the
 * result will be 0b00110011 (the value 0b11 is placed at index 0 and index 2,
 * with each index taking 2 bits).
 */
const fn fill_at_indexes(indexes: &[u8], value: u32, bit_width: u8) -> u32 {
    let mut r: u32 = 0;
    let mut i = 0;
    while i < indexes.len() {
        r |= (value & ((1 << bit_width) - 1)) << (indexes[i] * bit_width);
        i += 1;
    }
    return r;
}

/**
 * Computes a bitmask that will be applied only to the given indexes with a
 * given bit width.
 *
 * This is equivalent to a call to fill_at_indexes(indexes, (1 << bit_width) -
 * 1, bit_width).
 *
 */
const fn compute_mask(indexes: &[u8], bit_width: u8) -> u32 {
    return fill_at_indexes(indexes, (1 << bit_width) - 1, bit_width);
}

const fn gpio_ospeedr_value_for_pins(pins: &[u8], speed: Speed) -> u32 {
    return fill_at_indexes(pins, speed as u32, 2);
}

const fn gpio_ospeedr_bitmask(pins: &[u8]) -> u32 {
    return compute_mask(pins, 2);
}

const fn gpio_moder_value_for_pins(pins: &[u8], mode: MODER0) -> u32 {
    return fill_at_indexes(pins, mode as u32, 2);
}

const fn gpio_moder_bitmask(pins: &[u8]) -> u32 {
    return compute_mask(pins, 2);
}

const fn gpio_pupdr_value_for_pins(pins: &[u8], mode: PUPDR0) -> u32 {
    return fill_at_indexes(pins, mode as u32, 2);
}

const fn gpio_pupdr_bitmask(pins: &[u8]) -> u32 {
    return compute_mask(pins, 2);
}

const fn gpio_otyper_value_for_pins(pins: &[u8], otype: OT0) -> u32 {
    return fill_at_indexes(pins, otype as u32, 1);
}

const fn gpio_otyper_bitmask(pins: &[u8]) -> u32 {
    return compute_mask(pins, 1);
}

const fn gpio_bsrr_half_value_for_pins(pins: &[u8]) -> u16 {
    return (compute_mask(pins, 1) & 0xffff) as u16;
}

const fn count_different_ports(ports: &[char]) -> usize {
    let mut count = 0;
    let mut i = 0;

    while i < ports.len() {
        let c = ports[i];
        let mut seen_before = false;
        let mut j = 0;
        while j < i {
            if ports[j] == c {
                seen_before = true;
                break;
            }
            j += 1;
        }

        if !seen_before {
            count += 1;
        }

        i += 1;
    }

    count
}

const fn filter_pins_by_port(pins: &[(char, u8)], port: char) -> OwnedSlice<u8, 256> {
    let mut result = [0u8; 256];
    let mut idx = 0;
    let mut i = 0;
    while i < pins.len() {
        if pins[i].0 == port {
            result[idx] = pins[i].1;
            idx += 1;
        }
        i += 1;
    }

    OwnedSlice::new(result, idx)
}


const fn gpiox<const P: char>() -> *const gpioa::RegisterBlock {
    match P {
        'A' => stm32f4xx_hal::pac::GPIOA::ptr(),
        'B' => stm32f4xx_hal::pac::GPIOB::ptr() as _,
        'C' => stm32f4xx_hal::pac::GPIOC::ptr() as _,
        'D' => stm32f4xx_hal::pac::GPIOD::ptr() as _,
        _ => panic!("Unknown GPIO port"),
    }
}


const unsafe fn bsrr_ptr(base_ptr: *mut u32, new_value: bool) -> *mut u16 {
    unsafe {
        if new_value {
            base_ptr as *mut u16
        } else {
            // If we're clearing the value in the GPIO pin, we only write to the upper half of the BSRR register.
            (base_ptr as *mut u16).add(1)
        }
    }
}

#[inline(always)]
const fn has_pin_in_port(pins: &[(char, u8)], port: char) -> bool {
    return filter_pins_by_port(pins, port).len > 0;
}

pub struct PinSetRead<PS: PinSet> {
    _ps: PhantomData<PS>,
    values: [u16; DEV_PORT_COUNT]
}

impl<PS: PinSet> PinSetRead<PS> where [(); PS::NUM_PINS]:  {
    #[inline(always)]
    pub fn get_all(&self) -> [bool; PS::NUM_PINS] {
        let mut buf = [false; PS::NUM_PINS];
        let mut i = 0;
        for (pin_port, pin_idx) in PS::PIN_REFS {
            let port_value = self.values[*pin_port as usize - 'A' as usize];
            let pin_state = (port_value & (1 << pin_idx)) > 0;
            buf[i] = pin_state;
            i+=1;
        }
        buf
    }
}

macro_rules! pin_set_impl {
    ($npins:literal; $($dev_port_lit:literal $dev_port_gpio:ident)*) => {
        seq_macro::seq!(i in 0..$npins {
            pin_set_impl!(@ $npins; #(P~i N~i)*; $($dev_port_lit $dev_port_gpio)*);
        });
    };

    (@ $npins:literal; $($port_const:ident $pin_const:ident)*; $($dev_port_lit:literal $dev_port_gpio:ident)*) => {
        impl <$(const $port_const: char, const $pin_const: u8),*> PinSetSized<$npins> for ($(DynamicPin<$port_const, $pin_const>,)*) {
        }

        impl <$(const $port_const: char, const $pin_const: u8),*> PinSet for ($(DynamicPin<$port_const, $pin_const>,)*) {
            const NUM_PINS: usize = $npins;
            const NUM_PORTS: usize = count_different_ports(&[$($port_const),*]);
            const PIN_REFS: &'static [(char, u8)] = &[
                $(
                    ($port_const, $pin_const),
                )*
            ];

            #[inline(always)]
            fn make_input_pull_up(&mut self) {
                $(
                    if has_pin_in_port(Self::PIN_REFS, $dev_port_lit) {
                        let moder_bitmask = gpio_moder_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let moder_value = gpio_moder_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), MODER0::Input);

                        let pupdr_bitmask = gpio_pupdr_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let pupdr_value = gpio_pupdr_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), PUPDR0::PullUp);

                        unsafe {
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().moder().modify(|r, w| w.bits((r.bits() & (!moder_bitmask)) | moder_value));
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().pupdr().modify(|r, w| w.bits((r.bits() & (!pupdr_bitmask)) | pupdr_value));
                        }
                    }
                )*
            }

            #[inline(always)]
            fn make_input_pull_down(&mut self) {
                $(
                    if has_pin_in_port(Self::PIN_REFS, $dev_port_lit) {
                        let moder_bitmask = gpio_moder_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let moder_value = gpio_moder_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), MODER0::Input);

                        let pupdr_bitmask = gpio_pupdr_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let pupdr_value = gpio_pupdr_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), PUPDR0::PullDown);

                        unsafe {
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().moder().modify(|r, w| w.bits((r.bits() & (!moder_bitmask)) | moder_value));
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().pupdr().modify(|r, w| w.bits((r.bits() & (!pupdr_bitmask)) | pupdr_value));
                        }
                    }
                )*
            }

            #[inline(always)]
            fn make_input_floating(&mut self) {
                $(
                    if has_pin_in_port(Self::PIN_REFS, $dev_port_lit) {
                        let moder_bitmask = gpio_moder_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let moder_value = gpio_moder_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), MODER0::Input);

                        let pupdr_bitmask = gpio_pupdr_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let pupdr_value = gpio_pupdr_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), PUPDR0::Floating);

                        unsafe {
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().moder().modify(|r, w| w.bits((r.bits() & (!moder_bitmask)) | moder_value));
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().pupdr().modify(|r, w| w.bits((r.bits() & (!pupdr_bitmask)) | pupdr_value));
                        }
                    }
                )*
            }

            #[inline(always)]
            fn make_output_push_pull(&mut self) {
                $(
                    if has_pin_in_port(Self::PIN_REFS, $dev_port_lit) {
                        let moder_bitmask = gpio_moder_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let moder_value = gpio_moder_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), MODER0::Output);

                        let otyper_bitmask = gpio_otyper_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let otyper_value = gpio_otyper_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), OT0::PushPull);


                        unsafe {
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().moder().modify(|r, w| w.bits((r.bits() & (!moder_bitmask)) | moder_value));
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().otyper().modify(|r, w| w.bits((r.bits() & (!otyper_bitmask)) | otyper_value));
                        }
                    }
                )*
            }

            #[inline(always)]
            fn make_output_open_drain(&mut self) {
                $(
                    if has_pin_in_port(Self::PIN_REFS, $dev_port_lit) {
                        let moder_bitmask = gpio_moder_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let moder_value = gpio_moder_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), MODER0::Output);

                        let otyper_bitmask = gpio_otyper_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let otyper_value = gpio_otyper_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), OT0::OpenDrain);


                        unsafe {
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().moder().modify(|r, w| w.bits((r.bits() & (!moder_bitmask)) | moder_value));
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().otyper().modify(|r, w| w.bits((r.bits() & (!otyper_bitmask)) | otyper_value));
                        }
                    }
                )*
            }

            #[inline(always)]
            fn read(&self) -> PinSetRead<Self> where Self: Sized {
                let mut values = [0u16; DEV_PORT_COUNT];
                $(
                    if has_pin_in_port(Self::PIN_REFS, $dev_port_lit) {
                        values[${index()}] = (unsafe { stm32f4xx_hal::pac::$dev_port_gpio::steal().idr().read().bits()} & 0xffff) as u16;
                    }
                )*

                PinSetRead { _ps: PhantomData, values }
            }

            #[inline(always)]
            fn write_all(&mut self, new_value: bool) {
                $(
                    if has_pin_in_port(Self::PIN_REFS, $dev_port_lit) {
                        unsafe {
                            *bsrr_ptr(stm32f4xx_hal::pac::$dev_port_gpio::steal().bsrr().as_ptr(), new_value) = gpio_bsrr_half_value_for_pins(
                                filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref()
                            );
                        }
                    }
                )*
            }

            #[inline(always)]
            fn write_single(&mut self, index: u32, new_value: bool) {
                $(
                    if index == ${index()} {
                        unsafe {
                            *bsrr_ptr((&*gpiox::<$port_const>()).bsrr().as_ptr(), new_value) = 1 << $pin_const;
                        }
                    }
                )*
            }

            #[inline(always)]
            fn set_speed(&mut self, speed: Speed) {
                $(
                    if has_pin_in_port(Self::PIN_REFS, $dev_port_lit) {
                        let ospeedr_bitmask = gpio_ospeedr_bitmask(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref());
                        let ospeedr_value = gpio_ospeedr_value_for_pins(filter_pins_by_port(Self::PIN_REFS, $dev_port_lit).get_ref(), speed);

                        unsafe {
                            stm32f4xx_hal::pac::$dev_port_gpio::steal().ospeedr().modify(|r, w| w.bits((r.bits() & (!ospeedr_bitmask)) | ospeedr_value));
                        }
                    }
                 )*
            }
        }
    };
}

const DEV_PORT_COUNT: usize = 4;
pin_set_impl!(1; 'A' GPIOA 'B' GPIOB 'C' GPIOC 'D' GPIOD);
pin_set_impl!(2; 'A' GPIOA 'B' GPIOB 'C' GPIOC 'D' GPIOD);
pin_set_impl!(3; 'A' GPIOA 'B' GPIOB 'C' GPIOC 'D' GPIOD);
pin_set_impl!(4; 'A' GPIOA 'B' GPIOB 'C' GPIOC 'D' GPIOD);
pin_set_impl!(5; 'A' GPIOA 'B' GPIOB 'C' GPIOC 'D' GPIOD);
pin_set_impl!(6; 'A' GPIOA 'B' GPIOB 'C' GPIOC 'D' GPIOD);
pin_set_impl!(7; 'A' GPIOA 'B' GPIOB 'C' GPIOC 'D' GPIOD);
pin_set_impl!(8; 'A' GPIOA 'B' GPIOB 'C' GPIOC 'D' GPIOD);
pin_set_impl!(9; 'A' GPIOA 'B' GPIOB 'C' GPIOC 'D' GPIOD);
pin_set_impl!(10; 'A' GPIOA 'B' GPIOB 'C' GPIOC 'D' GPIOD);

use core::{arch::asm, marker::PhantomData, sync::atomic::{fence, Ordering}};

use log::info;
use seq_macro::seq;
use stm32f4xx_hal::{gpio::{Input, Output, Pin, PinState, PushPull}, hal::digital::{InputPin, OutputPin}, pac::GPIOA, time::Hertz};
use cortex_m::peripheral::DWT;

use crate::dev_info;

use super::gpio::{GpioPort, GpioX};
macro_rules! output_pins_impl {
    (@ $npins:literal, $($port_const:ident $pin_const:ident)*, $($pin_lit:literal)*) => {
        impl <$(const $port_const: char, const $pin_const: u8),*> OutputPins<$npins> for (
            $(
                 Pin<$port_const, $pin_const, Output<PushPull>>
            ),*
        ) {
            fn set_state(&mut self, col: u8, value: PinState) {
                seq_macro::seq!(N in 0..$npins {
                    match col {
                        #(
                            N => self.N.set_state(value),
                        )*
                        _ => panic!("Attempt to set state of a row pin out of bounds!")
                    }
                });
            }

            #[inline(always)]
            fn setup_pins(&mut self) {
                seq_macro::seq!(N in 0..$npins {
                    // Sets the OSPEEDR registers to a reasonable
                    // value. With this, a STM32F411 with Vdd > 2.7V
                    // should take around 10 ns to raise a pin.
                    self.N.set_speed(stm32f4xx_hal::gpio::Speed::Medium);

                    // Matrix scan uses active low for determining
                    // whether a key is pressed. Therefore, output
                    // pins will be high by default and pulled low
                    // individually when matrix is scanned.
                    self.N.set_state(stm32f4xx_hal::gpio::PinState::High);
                });
            }
        }
    };

    ($($npins:literal),*) => {
        $(
        seq_macro::seq!(i in 0..$npins {
            output_pins_impl!(@ $npins, #(P~i N~i)*, #(i)*);
        });
        )*
    }
}

macro_rules! into_pins_with_same_port_impl {
    ($($npins:literal),*) => {
        $(
           seq_macro::seq!(i in 0..$npins {
               impl<const PORT: char, #(const N~i: u8,)*> IntoKeyMatrixInputPinsWithSamePort for (#(Pin<PORT, N~i, Input>,)*) {
                   type Output = PinsWithSamePort<(#(Pin<PORT, N~i, Input>,)*)>;

                   fn into_input_pins_with_same_port(self) -> Self::Output {
                       Self::Output {
                           pins: self
                       }
                   }
               }
           });
        )*
    };
}

macro_rules! input_pins_same_port_impl {
    ($($npins:literal),*) => {
        $(
          seq_macro::seq!(i in 0..$npins {
              impl<const PORT: char, #(const N~i: u8,)*> InputPins<$npins> for PinsWithSamePort<(#(Pin<PORT, N~i, Input>,)*)> where GpioX<PORT>: GpioPort, SamePortReadResults<Self>: InputRead {
                  type ReadResult = SamePortReadResults<Self>;

                  fn read_inputs(&self) -> Self::ReadResult {
                      SamePortReadResults {
                          read_value: unsafe {
                              // SAFETY: SamePortReadResults will make
                              // sure only owned pins that are known
                              // to be in a input mode are read.

                              // Negate the value, since a low signal
                              // means that the input is on.
                              !GpioX::<PORT>::idr_value()
                          },
                          _data: PhantomData
                      }
                  }

                  fn setup_pins(&mut self) {
                      #(
                          self.pins.i.set_internal_resistor(stm32f4xx_hal::gpio::Pull::Up);
                      )*
                  }
              }

              impl<const PORT: char, #(const N~i: u8,)*> InputRead for SamePortReadResults<PinsWithSamePort<(#(Pin<PORT, N~i, Input>,)*)>> {
                  fn is_on(&self, row: u8) -> bool {
                      let mask = match row {
                          #(
                              i => 1 << N~i,
                          )*
                          _ => panic!("Out of bounds!")
                      };

                      (self.read_value & mask) > 0
                  }
              }
          });
        )*
    };
}

// Implement [`OutputPins`] for different number of pins.
output_pins_impl!(2, 3, 4);

// Implement [`IntoKeyMatrixInputPinsWithSamePort`] for different
// number of pins. This allows converting a tuple of pins into [`PinsWithSamePort<T>`],
// so that all pins can be read at once from a single register.
into_pins_with_same_port_impl!(2, 3, 4);

// Implement [`InputPins`] for different number of pins, that are
// located at the same GPIO port.
input_pins_same_port_impl!(2, 3, 4);

/// Holds the result of reading a GPIO register that holds the input
/// value of multiple pins, defined by the type T.
pub struct SamePortReadResults<T> {
    read_value: u32,
    _data: PhantomData<T>
}

/// Represents a type that can be converted into a type (that conforms
/// to [`InputPins`]), that can read input values from different pins
/// from a single GPIO register.
pub trait IntoKeyMatrixInputPinsWithSamePort {
    type Output;

    fn into_input_pins_with_same_port(self) -> Self::Output;
}

/// Represents a set of input ports that are located in the same GPIO
/// port. This struct implements the [`KeyMatrixInputPins`] trait,
/// reading all the input values in all the pins in the same cycle!
pub struct PinsWithSamePort<T> {
    pins: T
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum KeyState {
    Released = 0,
    Pressed = 1,
}

impl KeyState {
    pub const fn from_bool(value: bool) -> KeyState {
        match value {
            true => KeyState::Pressed,
            false => KeyState::Released
        }
    }
}

impl Default for KeyState {
    fn default() -> Self {
        KeyState::Released
    }
}

/// Represents a type that holds a read of the status of multiple
/// input pins.
pub trait InputRead {
    fn is_on(&self, row: u8) -> bool;
}

/// Represents a type that holds a set of input pins whose value can
/// be read all at once. These pins represents the rows of the matrix.
pub trait InputPins<const C: u8> {
    type ReadResult: InputRead;

    fn read_inputs(&self) -> Self::ReadResult;
    fn setup_pins(&mut self);
}

/// Represents a type that holds a set of pins that can be turned on
/// or off individually for scanning a key matrix. These pins
/// represents the columns of the matrix.
pub trait OutputPins<const C: u8> {
    fn set_state(&mut self, col: u8, value: PinState);
    fn setup_pins(&mut self);
}

// TODO Think on moving this to its own abstraction (e.g BitMatrix or whatever shit)
pub trait MatrixLayout {
    type RowDataType: Default + Copy;

    fn set_state(elem: &mut Self::RowDataType, col: u8, value: KeyState);
    fn get_state(elem: Self::RowDataType, col: u8) -> KeyState;
}

pub struct Layout<const ROWS: u8> {}

impl MatrixLayout for Layout<4> {
    type RowDataType = u8;

    fn set_state(elem: &mut Self::RowDataType, col: u8, value: KeyState) {
        match value { // TODO I guess that bit banding doesn't really worth at this point?
            KeyState::Released => *elem &= !(1 << col),
            KeyState::Pressed => *elem |= 1 << col,
        }
    }

    fn get_state(elem: Self::RowDataType, col: u8) -> KeyState {
        KeyState::from_bool((elem & (1 << col)) > 0)
    }

}

pub trait Debounce<const ROWS: u8, const COLS: u8> {
    fn debounce(&mut self, row: u8, col: u8, current_millis: u32, prev_state: KeyState, last_read_state: KeyState) -> KeyState;
}

pub struct NoDebouncer {}
impl<const ROWS: u8, const COLS: u8> Debounce<ROWS, COLS> for NoDebouncer {
    fn debounce(&mut self, _row: u8, _col: u8, _current_millis: u32, _prev_state: KeyState, last_read_state: KeyState) -> KeyState {
        last_read_state
    }
}

// Following a similar naming than QMK
pub struct DebouncerEagerPerKey<const ROWS: u8, const COLS: u8, const DEBOUNCE_MILLIS: u8> where [(); (ROWS as usize) * (COLS as usize)]: {
    // Holds the last time, in millis, when each key was pressed,
    // except if its value is 0xff. In such case, it is considered
    // that the last event for the referred key happened a long time
    // ago, and next event coming from this key can be applied. This
    // is a trick that QMK uses for handling the fact that a u8 value
    // that represents millis will constantly be wrapping, and
    // therefore, zero is a perfectly valid value. Therefore, instead
    // of zero, we reserve the value 0xff as a special value for
    // represent this case.
    last_change_millis: [u8; (ROWS as usize) * (COLS as usize)]
}

impl<const ROWS: u8, const COLS: u8, const DEBOUNCE_MILLIS: u8> DebouncerEagerPerKey<ROWS, COLS, DEBOUNCE_MILLIS> where [(); (ROWS as usize) * (COLS as usize)]:  {
    const fn assert_debounce_millis_in_range(millis: u8) {
        assert!(millis < 255, "Debounce time cannot be greater than 254 ms!");
    }

    pub fn new() -> Self {
        const {
            Self::assert_debounce_millis_in_range(DEBOUNCE_MILLIS)
        };

        DebouncerEagerPerKey { last_change_millis: [0xffu8; (ROWS as usize) * (COLS as usize)] }
    }

    pub const fn diff_time(newer: u8, older: u8) -> u8 {
        if newer >= older {
            newer - older
        } else {
            255 - older + newer
        }
    }
}

impl<const ROWS: u8, const COLS: u8, const DEBOUNCE_MILLIS: u8> Debounce<ROWS, COLS> for DebouncerEagerPerKey<ROWS, COLS, DEBOUNCE_MILLIS> where [(); (ROWS as usize) * (COLS as usize)]:  {
    fn debounce(&mut self, row: u8, col: u8, current_millis: u32, prev_state: KeyState, last_read_state: KeyState) -> KeyState {
        let wrapped_millis = (current_millis % 254) as u8;
        let last_change_ms = &mut self.last_change_millis[row as usize * COLS as usize + col as usize];
        if *last_change_ms != 0xff {
            if Self::diff_time(wrapped_millis, *last_change_ms) < DEBOUNCE_MILLIS {
                // The last update was recent. Just ignore everything.
                return prev_state;
            } else {
                // The debounce time have already passed. Mark it as such, and continue.
                dev_info!("Debounce time over for {}; {} ({} ms). Because {} - {} = {}", row, col, current_millis, wrapped_millis, *last_change_ms, Self::diff_time(wrapped_millis, *last_change_ms));
                *last_change_ms = 0xff;
            }
        }

        if prev_state != last_read_state {
            // If there has been any change, report the change, and
            // store the time when it happened.
            dev_info!("Debounce time set in {} ms after {:?}", current_millis, last_read_state);
            *last_change_ms = wrapped_millis;
        }
        return last_read_state

    }
}

pub struct KeyMatrix<const ROWS: u8, const COLS: u8, IN, OUT, D> where [(); ROWS as usize]:, Layout<ROWS>: MatrixLayout {
    buf: [<Layout<ROWS> as MatrixLayout>::RowDataType; ROWS as usize],
    input_pins: IN,
    output_pins: OUT,
    debouncer: D,
    sysclk_freq: Hertz
}

impl<const ROWS: u8, const COLS: u8, IN, OUT, D> KeyMatrix<ROWS, COLS, IN, OUT, D> where [(); ROWS as usize]:, IN: InputPins<ROWS>, OUT: OutputPins<COLS>, D: Debounce<ROWS, COLS>, Layout<ROWS>: MatrixLayout {
    pub fn new(sysclk_freq: Hertz, mut input_pins: IN, mut output_pins: OUT, debouncer: D) -> Self {
        input_pins.setup_pins();
        output_pins.setup_pins();

        Self {
            buf: [Default::default(); ROWS as usize],
            input_pins,
            output_pins,
            debouncer,
            sysclk_freq
        }
    }

    #[inline(always)]
    pub fn get_key_state(&self, row: u8, col: u8) -> KeyState {
        Layout::<ROWS>::get_state(self.buf[row as usize], col)
    }

    #[inline(always)]
    pub fn set_key_state(&mut self, row: u8, col: u8, state: KeyState) {
        Layout::<ROWS>::set_state(&mut self.buf[row as usize], col, state);
    }

    pub fn scan_matrix(&mut self) {
        let current_millis = ((DWT::cycle_count() as u64) * 1000 / self.sysclk_freq.raw() as u64) as u32;

        for col in 0..COLS {
            self.output_pins.set_state(col, PinState::Low);
            fence(Ordering::SeqCst);
            unsafe {
                // Wait a couple of cycles to let the gpio pin
                // stabilize. I guess this should take around... 10 ns
                // with OSPEEDR set to medium. So at 96 MHz, two dummy
                // instructions are more than enough.
                asm!(
                    "and r1, r1",
                    "and r1, r1",
                );
            }

            let inputs: IN::ReadResult = self.input_pins.read_inputs();
            fence(Ordering::SeqCst);
            self.output_pins.set_state(col, PinState::High);

            // This section should be already enough to give some time to the column pin to go low.
            for row in 0..ROWS {
                let new_state = KeyState::from_bool(inputs.is_on(row));
                let prev_state = self.get_key_state(row, col);
                let effective_state = self.debouncer.debounce(row, col, current_millis, prev_state, new_state);
                if effective_state != prev_state {
                    self.set_key_state(row, col, effective_state);
                    info!("{:?} ({}; {}) ({} ms)", effective_state, row, col, current_millis);
                }
            }
        }

    }
}

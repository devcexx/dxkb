use core::{
    arch::asm,
    marker::PhantomData,
    sync::atomic::{fence, Ordering},
};

use cortex_m::peripheral::DWT;
use stm32f4xx_hal::{
    gpio::{Input, Output, Pin, PinState, PushPull},
    time::Hertz,
};

use dxkb_common::{dev_info, dev_trace, util::{BitMatrix, BitMatrixLayout, ColBitMatrixLayout}, KeyState};

use super::gpio::{GpioPort, GpioX};

// TODO Replace this macros with crabtime functions.
macro_rules! output_pins_impl {
    (@ $npins:literal, $($port_const:ident $pin_const:ident)*, $($pin_lit:literal)*) => {
        impl <$(const $port_const: char, const $pin_const: u8),*> Pins for (
            $(
                 Pin<$port_const, $pin_const, Output<PushPull>>
            ),*
        ) {
            const COUNT: u8 = $npins;
        }

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
               impl<const PORT: char, #(const N~i: u8,)*> IntoInputPinsWithSamePort for (#(Pin<PORT, N~i, Input>,)*) {
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
              impl<const PORT: char, #(const N~i: u8,)*> Pins for PinsWithSamePort<(#(Pin<PORT, N~i, Input>,)*)> {
                  const COUNT: u8 = $npins;
              }

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
                  fn is_on(&self, pin_index: u8) -> bool {
                      let mask = match pin_index {
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

// Implement [`IntoInputPinsWithSamePort`] for different
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
    _data: PhantomData<T>,
}

/// Represents a type that can be converted into a type (that conforms
/// to [`InputPins`]), that can read input values from different pins
/// from a single GPIO register.
pub trait IntoInputPinsWithSamePort {
    type Output;

    fn into_input_pins_with_same_port(self) -> Self::Output;
}

/// Represents a set of input ports that are located in the same GPIO
/// port. This struct implements the [`InputPins`] trait,
/// reading all the input values in all the pins in the same cycle!
pub struct PinsWithSamePort<T> {
    pins: T,
}

/// Represents a type that holds a read of the status of multiple
/// input pins.
pub trait InputRead {
    fn is_on(&self, pin_index: u8) -> bool;
}

pub trait Pins {
    const COUNT: u8;
}

/// Represents a type that holds a set of input pins whose value can
/// be read all at once.
pub trait InputPins<const C: u8>: Pins {
    type ReadResult: InputRead;

    fn read_inputs(&self) -> Self::ReadResult;
    fn setup_pins(&mut self);
}

/// Represents a type that holds a set of pins that can be turned on
/// or off individually for scanning a key matrix.
pub trait OutputPins<const C: u8>: Pins {
    fn set_state(&mut self, col: u8, value: PinState);
    fn setup_pins(&mut self);
}

/// Represents a type that is able to debounce the input signal
/// generated by a button, attempting to remove the noise generated by
/// the effect of pressing or unpressing it.
pub trait Debounce<const ROWS: u8, const COLS: u8> {
    fn debounce(
        &mut self,
        row: u8,
        col: u8,
        current_millis: u32,
        prev_state: KeyState,
        last_read_state: KeyState,
    ) -> KeyState;
}

/// A debounce strategy where no debounce is done. Button status is
/// passed to the matrix as it is coming from the wire.
pub struct NoDebouncer {}
impl<const ROWS: u8, const COLS: u8> Debounce<ROWS, COLS> for NoDebouncer {
    fn debounce(
        &mut self,
        _row: u8,
        _col: u8,
        _current_millis: u32,
        _prev_state: KeyState,
        last_read_state: KeyState,
    ) -> KeyState {
        last_read_state
    }
}

// Following a similar naming than QMK
/// Debounce strategy in which a level change in the wire immediately
/// triggers a change in the button state (pressed or unpressed,
/// depending on the new level), and any other change in the wire for
/// that specific button is ignored for the following
/// `DEBOUNCE_MILLIS` milliseconds. For reducing the memory
/// footprint, the maximum debounce time is limited to 254 ms.
pub struct DebouncerEagerPerKey<const ROWS: u8, const COLS: u8, const DEBOUNCE_MILLIS: u8>
where
    [(); (ROWS as usize) * (COLS as usize)]:,
{
    // Holds the last time, in millis, when each key was pressed,
    // except if its value is 0xff. In such case, it is considered
    // that the last event for the referred key happened a long time
    // ago, and next event coming from this key can be applied. This
    // is a trick that QMK uses for handling the fact that a u8 value
    // that represents millis will constantly be wrapping, and
    // therefore, zero is a perfectly valid value. Therefore, instead
    // of zero, we reserve the value 0xff as a special value for
    // represent this case.
    last_change_millis: [u8; (ROWS as usize) * (COLS as usize)],
}

impl<const ROWS: u8, const COLS: u8, const DEBOUNCE_MILLIS: u8>
    DebouncerEagerPerKey<ROWS, COLS, DEBOUNCE_MILLIS>
where
    [(); (ROWS as usize) * (COLS as usize)]:,
{
    const fn assert_debounce_millis_in_range(millis: u8) {
        assert!(millis < 255, "Debounce time cannot be greater than 254 ms!");
    }

    pub fn new() -> Self {
        const { Self::assert_debounce_millis_in_range(DEBOUNCE_MILLIS) };

        DebouncerEagerPerKey {
            last_change_millis: [0xffu8; (ROWS as usize) * (COLS as usize)],
        }
    }

    pub const fn diff_time(newer: u8, older: u8) -> u8 {
        if newer >= older {
            newer - older
        } else {
            255 - older + newer
        }
    }
}

impl<const ROWS: u8, const COLS: u8, const DEBOUNCE_MILLIS: u8> Debounce<ROWS, COLS>
    for DebouncerEagerPerKey<ROWS, COLS, DEBOUNCE_MILLIS>
where
    [(); (ROWS as usize) * (COLS as usize)]:,
{
    fn debounce(
        &mut self,
        row: u8,
        col: u8,
        current_millis: u32,
        prev_state: KeyState,
        last_read_state: KeyState,
    ) -> KeyState {
        let wrapped_millis = (current_millis % 254) as u8;
        let last_change_ms =
            &mut self.last_change_millis[row as usize * COLS as usize + col as usize];
        if *last_change_ms != 0xff {
            if Self::diff_time(wrapped_millis, *last_change_ms) < DEBOUNCE_MILLIS {
                // The last update was recent. Just ignore everything.
                return prev_state;
            } else {
                // The debounce time have already passed. Mark it as such, and continue.
                dev_trace!(
                    "Debounce time over for {}; {} ({} ms). Because {} - {} = {}",
                    row,
                    col,
                    current_millis,
                    wrapped_millis,
                    *last_change_ms,
                    Self::diff_time(wrapped_millis, *last_change_ms)
                );
                *last_change_ms = 0xff;
            }
        }

        if prev_state != last_read_state {
            // If there has been any change, report the change, and
            // store the time when it happened.
            dev_trace!(
                "Debounce time set in {} ms after {:?}",
                current_millis,
                last_read_state
            );
            *last_change_ms = wrapped_millis;
        }
        return last_read_state;
    }
}

/// Represents a type that determines the way in which a key matrix is
/// scanned. It allows to dynamically determine which will be the type
/// of the input pins and the output pins depending on the direction
/// in which the matrix should be scanned according to the type that
/// conforms to this trait. See [`ColumnScan`] and [`RowScan`] for
/// more info.
pub trait MatrixScan<const ROWS: u8, const COLS: u8, RowPins, ColPins>
{
    type InPins: Pins;
    type OutPins: Pins;

    fn translate_pins(rows: RowPins, cols: ColPins) -> (Self::InPins, Self::OutPins);
    fn translate_indexes(input_pin_index: u8, output_pin_index: u8) -> (u8, u8);
}

/// A type of matrix scan in which the column pins are individually
/// selected, and the row pins are read at once. This should be the
/// selected matrix scan type if your board setup has diodes connected
/// from the rows to the columns.
pub struct ColumnScan {}

impl<const ROWS: u8, const COLS: u8, RowPins, ColPins> MatrixScan<ROWS, COLS, RowPins, ColPins>
    for ColumnScan
where
    RowPins: InputPins<ROWS>,
    ColPins: OutputPins<COLS>,
{
    type InPins = RowPins;
    type OutPins = ColPins;

    fn translate_pins(rows: RowPins, cols: ColPins) -> (Self::InPins, Self::OutPins) {
        (rows, cols)
    }

    fn translate_indexes(input_pin_index: u8, output_pin_index: u8) -> (u8, u8) {
        (input_pin_index, output_pin_index)
    }
}

/// A type of matrix scan in which the row pins are individually
/// selected, and the column pins are read at once. This should be the
/// selected matrix scan type if your board setup has diodes connected
/// from the columns to the rows.
pub struct RowScan {}

impl<const ROWS: u8, const COLS: u8, RowPins, ColPins> MatrixScan<ROWS, COLS, RowPins, ColPins>
    for RowScan
where
    RowPins: OutputPins<ROWS>,
    ColPins: InputPins<COLS>,
{
    type InPins = ColPins;
    type OutPins = RowPins;

    fn translate_pins(rows: RowPins, cols: ColPins) -> (Self::InPins, Self::OutPins) {
        (cols, rows)
    }

    fn translate_indexes(input_pin_index: u8, output_pin_index: u8) -> (u8, u8) {
        (output_pin_index, input_pin_index)
    }
}

pub trait KeyMatrixLike<const ROWS: u8, const COLS: u8> {
    fn get_key_state(&self, row: u8, col: u8) -> KeyState;
    fn set_key_state(&mut self, row: u8, col: u8, state: KeyState);

    /// Scans the current status of the key matrix, returning true if
    /// something has changed from the past scan.
    fn scan_matrix(&mut self) -> bool {
        self.scan_matrix_act(|_, _, _| {})
    }

    /// Scans the current status of the key matrix, returning true if
    /// something has changed from the past scan. The function
    /// `changed_fn` will be executed for each change detected in the
    /// matrix.
    fn scan_matrix_act<F: FnMut(u8, u8, KeyState) -> ()>(&mut self, changed_fn: F) -> bool;
}

/// A key matrix, constructed from the pins that forms the rows and
/// the columns of the matrix. In this matrix, a key is considered
/// pressed when it is active low.  More information about how key
/// matrices work:
/// <https://pcbheaven.com/wikipages/How_Key_Matrices_Works/>.
/// <br />
/// <br />
/// The definition of the key matrix is completed with the following
/// generics:
///  - `ROWS`: The number of rows in the matrix.
///  - `COLS`: The number of columns in the matrix.
///  - `RowPins`: The type that represents the row pins of the matrix.
///     They're generally represented as a tuple of pins.
///     Pin direction (input or output) depends on the matrix scan type S.
///  - `ColPins`: The type that represents the column pins of the matrix.
///    They're generally represented as a tuple of pins.
///    Pin direction (input or output) depends on the matrix scan type S.
///  - `S`: The type that indicates how the matrix will be scanned.
///    It should be set to either one of the following types:
///    - [`ColumnScan`]: The columns of the matrix are selected one by one,
///      and the rows are read all at once. In this mode, RowPins needs to
///      hold input pins, and ColPins needs to hold output pins.
///    - [`RowScan`]: The rows of the matrix are selected one by one,
///      and the columns are read all at once. In this mode, RowPins needs to
///      hold output pins, and ColPins needs to hold input pins.

pub struct KeyMatrix<const ROWS: u8, const COLS: u8, RowPins, ColPins, S, D>
where
    [(); ROWS as usize]:,
    S: MatrixScan<ROWS, COLS, RowPins, ColPins>,
    ColBitMatrixLayout<COLS>: BitMatrixLayout,
{
    matrix: BitMatrix<{ROWS as usize}, COLS>,
    input_pins: S::InPins,
    output_pins: S::OutPins,
    debouncer: D,
    sysclk_freq: Hertz, // TODO Change by usage of Clock trait
}

impl<const ROWS: u8, const COLS: u8, RowPins, ColPins, S, D>
    KeyMatrix<ROWS, COLS, RowPins, ColPins, S, D>
where
    [(); ROWS as usize]:,
    S: MatrixScan<ROWS, COLS, RowPins, ColPins>,
    D: Debounce<ROWS, COLS>,
    ColBitMatrixLayout<COLS>: BitMatrixLayout,
    S::InPins: InputPins<{ S::InPins::COUNT }>,
    S::OutPins: OutputPins<{ S::OutPins::COUNT }>,
{
    pub fn new(sysclk_freq: Hertz, rows: RowPins, cols: ColPins, debouncer: D) -> Self {
        let (mut in_pins, mut out_pins) = S::translate_pins(rows, cols);
        in_pins.setup_pins();
        out_pins.setup_pins();

        Self {
            matrix: BitMatrix::new(),
            input_pins: in_pins,
            output_pins: out_pins,
            debouncer,
            sysclk_freq,
        }
    }

}

impl<const ROWS: u8, const COLS: u8, RowPins, ColPins, S, D> KeyMatrixLike<ROWS, COLS> for KeyMatrix<ROWS, COLS, RowPins, ColPins, S, D> where
    [(); ROWS as usize]:,
    S: MatrixScan<ROWS, COLS, RowPins, ColPins>,
    D: Debounce<ROWS, COLS>,
    ColBitMatrixLayout<COLS>: BitMatrixLayout,
    S::InPins: InputPins<{ S::InPins::COUNT }>,
    S::OutPins: OutputPins<{ S::OutPins::COUNT }>, {

    #[inline(always)]
    fn get_key_state(&self, row: u8, col: u8) -> KeyState {
        KeyState::from_bool(self.matrix.get_value(row as usize, col))
    }

    #[inline(always)]
    fn set_key_state(&mut self, row: u8, col: u8, state: KeyState) {
        self.matrix.set_value(row as usize, col, state == KeyState::Pressed);
    }


    fn scan_matrix_act<F: FnMut(u8, u8, KeyState) -> ()>(&mut self, mut changed_fn: F) -> bool {
        let current_millis =
            ((DWT::cycle_count() as u64) * 1000 / self.sysclk_freq.raw() as u64) as u32;
        let mut has_changed = false;

        for output_pin_index in 0..S::OutPins::COUNT {
            self.output_pins.set_state(output_pin_index, PinState::Low);
            fence(Ordering::SeqCst);
            unsafe {
                // Wait a couple of cycles to let the gpio pin
                // stabilize. I guess this should take around... 10 ns
                // with OSPEEDR set to medium. So at 96 MHz, two dummy
                // instructions are more than enough.
                asm!("and r1, r1", "and r1, r1",);
            }

            let inputs = self.input_pins.read_inputs();
            fence(Ordering::SeqCst);
            self.output_pins.set_state(output_pin_index, PinState::High);

            // This section should be already enough to give some time to the column pin to go low.
            for input_pin_index in 0..S::InPins::COUNT {
                let new_state = KeyState::from_bool(inputs.is_on(input_pin_index));

                let (row, col) = S::translate_indexes(input_pin_index, output_pin_index);
                let prev_state = self.get_key_state(row, col);

                let effective_state =
                    self.debouncer
                        .debounce(row, col, current_millis, prev_state, new_state);
                if effective_state != prev_state {
                    has_changed = true;
                    self.set_key_state(row, col, effective_state);
                    changed_fn(row, col, effective_state);
                    dev_info!(
                        "{:?} ({}; {}) ({} ms)",
                        effective_state,
                        row,
                        col,
                        current_millis
                    );
                }
            }
        }

        has_changed
    }
}

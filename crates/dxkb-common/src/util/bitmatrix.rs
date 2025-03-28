use core::fmt::{Binary, Debug};

pub trait BitMatrixLayout {
    type ColType: Copy + Default + Debug + Binary;
    const ZERO: Self::ColType;

    /// Sets the state of the requested bit at the given column, and
    /// returns a value indicating whether the value has actually
    /// changed from the previous one.
    fn set_state(elem: &mut Self::ColType, col: u8, value: bool) -> bool;
    fn get_state(elem: Self::ColType, col: u8) -> bool;
}
pub struct ColBitMatrixLayout<const COLS: u8> {}


#[crabtime::function]
fn gen_bit_matrix_layout_impls() {
    for bits in 1..=128 {
        let typ = match bits {
            1..=8 => "u8",
            9..=16 => "u16",
            17..=32 => "u32",
            33..=64 => "u64",
            _ => "u128"
        };

        crabtime::output! {
            impl BitMatrixLayout for ColBitMatrixLayout<{{bits}}> {
                const ZERO: {{typ}} = 0;
                type ColType = {{typ}};

                #[inline(always)]
                fn set_state(elem: &mut Self::ColType, col: u8, value: bool) -> bool {
                    let prev = *elem;
                    if value {
                        *elem |= 1 << col;
                    } else {
                        *elem &= !(1 << col);
                    }

                    prev != *elem
                }

                #[inline(always)]
                fn get_state(elem: Self::ColType, col: u8) -> bool {
                    (elem & (1 << col)) > 0
                }
            }
        }
    }
}

gen_bit_matrix_layout_impls!();

#[derive(Debug)]
pub struct BitMatrix<const ROWS: usize, const COLS: u8> where ColBitMatrixLayout<COLS>: BitMatrixLayout {
    buf: [<ColBitMatrixLayout<COLS> as BitMatrixLayout>::ColType; ROWS]
}

impl<const ROWS: usize, const COLS: u8> BitMatrix<ROWS, COLS> where ColBitMatrixLayout<COLS>: BitMatrixLayout {
    pub const fn new() -> Self {
        Self {
            buf: [ColBitMatrixLayout::<COLS>::ZERO; ROWS]
        }
    }

    #[inline(always)]
    pub fn get_value(&self, row: usize, col: u8) -> bool {
        assert!(row < ROWS, "Row out of bounds");
        assert!(col < COLS, "Col out of bounds");

        <ColBitMatrixLayout<COLS> as BitMatrixLayout>::get_state(self.buf[row], col)
    }

    #[inline(always)]
    pub fn set_value(&mut self, row: usize, col: u8, value: bool) -> bool {
        assert!(row < ROWS, "Row out of bounds");
        assert!(col < COLS, "Col out of bounds");

        <ColBitMatrixLayout<COLS> as BitMatrixLayout>::set_state(&mut self.buf[row], col, value)
    }
}

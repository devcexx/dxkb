use core::{fmt::{Binary, Debug}, ops::Index, slice::SliceIndex};

use crate::dev_info;




pub trait BitMatrixLayout {
    type ColType: Copy + Default + Debug + Binary;

    fn set_state(elem: &mut Self::ColType, col: u8, value: bool);
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
                type ColType = {{typ}};

                fn set_state(elem: &mut Self::ColType, col: u8, value: bool) {
                    if value {
                        *elem |= 1 << col;
                    } else {
                        *elem &= !(1 << col);
                    }
                }

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
    pub fn new() -> Self {
        Self {
            buf: [Default::default(); ROWS]
        }
    }

    pub fn get_value(&self, row: usize, col: u8) -> bool {
        assert!(row < ROWS, "Row out of bounds");
        assert!(col < COLS, "Col out of bounds");

        <ColBitMatrixLayout<COLS> as BitMatrixLayout>::get_state(self.buf[row], col)
    }

    pub fn set_value(&mut self, row: usize, col: u8, value: bool) {
        assert!(row < ROWS, "Row out of bounds");
        assert!(col < COLS, "Col out of bounds");

        <ColBitMatrixLayout<COLS> as BitMatrixLayout>::set_state(&mut self.buf[row], col, value);
    }
}

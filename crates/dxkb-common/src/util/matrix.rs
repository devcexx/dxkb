use crate::util::Assert;

const MATRIX_SHAPE_ASSERT<const ROWS: u8, const COLS: u8>: () = const {
    assert!(ROWS > 0, "Dim must have at least one row");
    assert!(COLS > 0, "Dim must have at least one column");
};

pub struct MatrixShape<const ROWS: u8, const COLS: u8> where Assert<{MATRIX_SHAPE_ASSERT::<ROWS, COLS>}>:;
pub trait TMatrixShape {
    const ROWS: u8;
    const COLS: u8;
}

impl <const ROWS: u8, const COLS: u8> TMatrixShape for MatrixShape<ROWS, COLS> {
    const ROWS: u8 = ROWS;
    const COLS: u8 = COLS;
}

pub const MATRIX_ROWS<M: TMatrixShape>: u8 = M::ROWS;
pub const MATRIX_COLS<M: TMatrixShape>: u8 = M::COLS;
pub const MATRIX_SIZE_USIZE<M: TMatrixShape>: usize = MATRIX_ROWS::<M> as usize * MATRIX_COLS::<M> as usize;

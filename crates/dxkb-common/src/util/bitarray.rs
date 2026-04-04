use zerocopy::{FromBytes, Immutable, IntoBytes};

pub trait FieldWidth {
    const BIT_WIDTH: usize;
    const _ASSERT_BIT_WIDTH_OK: () = assert!(Self::BIT_WIDTH > 0 && Self::BIT_WIDTH <= 8, "Invalid bit width");

    /// Holds the number of entire fields that fit into a single byte. If the
    /// bit width does not divide 8 evenly, the last bits of the byte won't be
    /// used to hold a field, and therefore will be ignored.
    const FIELDS_PER_BYTE: usize = 8 / Self::BIT_WIDTH;
    type TField: Copy + Default;

    fn put(value: Self::TField, field_index: usize, ptr: &mut u8) -> Self::TField;
    fn get(field_index: usize, ptr: &u8) -> Self::TField;
}

pub struct OneBit;
pub struct TwoBits;

impl FieldWidth for OneBit {
    const BIT_WIDTH: usize = 1;
    type TField = bool;

    fn put(value: Self::TField, field_index: usize, ptr: &mut u8) -> Self::TField {
        let prev = *ptr;
        let mask = 1 << field_index;

        if value {
            *ptr |= mask;
        } else {
            *ptr &= !(1 << field_index);
        }

        (prev & mask) > 0
    }

    fn get(field_index: usize, ptr: &u8) -> Self::TField {
        (*ptr & (1 << field_index)) != 0
    }
}

impl FieldWidth for TwoBits {
    const BIT_WIDTH: usize = 2;
    type TField = u8;

    fn put(value: Self::TField, field_index: usize, ptr: &mut u8) -> Self::TField {
        let prev = *ptr;

        *ptr &= !(0b11 << (field_index * Self::BIT_WIDTH));
        *ptr |= (value & 3) << (field_index * Self::BIT_WIDTH);

        (prev >> field_index * Self::BIT_WIDTH) & 3
    }

    fn get(field_index: usize, ptr: &u8) -> Self::TField {
        (*ptr >> (field_index * Self::BIT_WIDTH)) & 3
    }
}

pub const fn bit_array_size<W: FieldWidth>(n: usize) -> usize {
    1 + ((n - 1) / W::FIELDS_PER_BYTE)
}

#[repr(transparent)]
#[derive(Clone, FromBytes, IntoBytes, Immutable, Debug)]
pub struct BitArray<W: FieldWidth, const N: usize>
where
    [(); bit_array_size::<W>(N)]:,
{
    buf: [u8; bit_array_size::<W>(N)],
}

impl <W: FieldWidth, const N: usize> Default for BitArray<W, N>
where
    [(); bit_array_size::<W>(N)]:,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<W: FieldWidth, const N: usize> BitArray<W, N>
where
    [(); bit_array_size::<W>(N)]:,
{
    pub const fn new() -> Self {
        Self {
            buf: [0; bit_array_size::<W>(N)],
        }
    }

    pub fn new_from_values(values: &[W::TField; N]) -> Self {
        let mut array = Self::new();
        for i in 0..N {
            array.put(i, values[i]);
        }

        array
    }

    fn assert_within_bounds(index: usize) {
        assert!(index < N, "Index out of bounds: {}", index);
    }

    pub unsafe fn put_unchecked(&mut self, index: usize, value: W::TField) -> W::TField {
        unsafe {
            let byte = index / W::FIELDS_PER_BYTE;
            let field_index = index % W::FIELDS_PER_BYTE;
            W::put(value, field_index, self.buf.get_unchecked_mut(byte))
        }
    }

    pub unsafe fn get_unchecked(&self, index: usize) -> W::TField {
        unsafe {
            let byte = index / W::FIELDS_PER_BYTE;
            let field_index = index % W::FIELDS_PER_BYTE;
            W::get(field_index, self.buf.get_unchecked(byte))
        }
    }

    #[inline]
    pub fn clear(&mut self, index: usize) -> W::TField {
        Self::assert_within_bounds(index);
        unsafe {
            self.put_unchecked(index, Default::default())
        }
    }

    #[inline]
    pub fn put(&mut self, index: usize, value: W::TField) -> W::TField {
        Self::assert_within_bounds(index);
        unsafe {
            // SAFETY: Bounds previously checked.
            self.put_unchecked(index, value)
        }
    }

    #[inline]
    pub fn get(&self, index: usize) -> W::TField {
        Self::assert_within_bounds(index);
        unsafe {
            // SAFETY: Bounds previously checked.
            self.get_unchecked(index)
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use zerocopy::IntoBytes;

    #[test]
    fn onebit_8_fields_store_and_retrieve() {
        let mut arr = BitArray::<OneBit, 8>::new();

        for i in 0..8 {
            assert_eq!(arr.get(i), false);
        }

        arr.put(0, true);
        arr.put(3, true);
        arr.put(7, true);

        assert_eq!(arr.get(0), true);
        assert_eq!(arr.get(1), false);
        assert_eq!(arr.get(2), false);
        assert_eq!(arr.get(3), true);
        assert_eq!(arr.get(4), false);
        assert_eq!(arr.get(5), false);
        assert_eq!(arr.get(6), false);
        assert_eq!(arr.get(7), true);
    }

    #[test]
    fn onebit_9_fields_crosses_byte_boundary() {
        let mut arr = BitArray::<OneBit, 9>::new();

        arr.put(7, true);
        assert_eq!(arr.get(7), true);
        assert_eq!(arr.get(8), false);

        arr.put(8, true);
        assert_eq!(arr.get(7), true);
        assert_eq!(arr.get(8), true);

        arr.put(7, false);
        assert_eq!(arr.get(7), false);
        assert_eq!(arr.get(8), true);
    }

    #[test]
    fn twobits_4_fields_store_and_retrieve() {
        let mut arr = BitArray::<TwoBits, 4>::new();

        arr.put(0, 0);
        arr.put(1, 1);
        arr.put(2, 2);
        arr.put(3, 3);

        assert_eq!(arr.get(0), 0);
        assert_eq!(arr.get(1), 1);
        assert_eq!(arr.get(2), 2);
        assert_eq!(arr.get(3), 3);
    }

    #[test]
    fn twobits_5_fields_crosses_byte_boundary() {
        let mut arr = BitArray::<TwoBits, 5>::new();

        arr.put(3, 3);
        assert_eq!(arr.get(3), 3);
        assert_eq!(arr.get(4), 0);

        arr.put(4, 2);
        assert_eq!(arr.get(3), 3);
        assert_eq!(arr.get(4), 2);

        arr.put(3, 1);
        assert_eq!(arr.get(3), 1);
        assert_eq!(arr.get(4), 2);
    }

    #[test]
    fn twobits_known_layout_fields_1_3_5_set_to_3() {
        let mut arr = BitArray::<TwoBits, 8>::new();
        arr.put(1, 0b11);
        arr.put(3, 0b11);
        arr.put(5, 0b11);

        let raw = arr.as_bytes();
        assert_eq!(raw, &[0xCC, 0x0C]);
    }

    #[test]
    fn new_from_values_onebit_exact_sequence() {
        let values = [true, false, true, true, false, false, true, false, true];
        let arr = BitArray::<OneBit, 9>::new_from_values(&values);

        for i in 0..values.len() {
            assert_eq!(arr.get(i), values[i]);
        }
    }

    #[test]
    fn new_from_values_twobits_exact_sequence() {
        let values = [0, 1, 2, 3, 3, 2, 1, 0];
        let arr = BitArray::<TwoBits, 8>::new_from_values(&values);

        for i in 0..values.len() {
            assert_eq!(arr.get(i), values[i]);
        }
    }

    #[test]
    fn put_returns_previous_and_updates_onebit() {
        let mut arr = BitArray::<OneBit, 8>::new();

        assert_eq!(arr.put(2, true), false);
        assert_eq!(arr.get(2), true);
        assert_eq!(arr.put(2, false), true);
        assert_eq!(arr.get(2), false);
    }

    #[test]
    fn put_returns_previous_and_updates_twobits() {
        let mut arr = BitArray::<TwoBits, 8>::new();

        assert_eq!(arr.put(4, 3), 0);
        assert_eq!(arr.get(4), 3);
        assert_eq!(arr.put(4, 1), 3);
        assert_eq!(arr.get(4), 1);
    }

    #[test]
    fn get_returns_latest_values_onebit() {
        let mut arr = BitArray::<OneBit, 17>::new();

        arr.put(0, true);
        arr.put(8, true);
        arr.put(16, true);
        arr.put(8, false);

        assert_eq!(arr.get(0), true);
        assert_eq!(arr.get(8), false);
        assert_eq!(arr.get(16), true);
    }

    #[test]
    fn get_returns_latest_values_twobits() {
        let mut arr = BitArray::<TwoBits, 9>::new();

        arr.put(0, 1);
        arr.put(4, 2);
        arr.put(8, 3);
        arr.put(4, 0);

        assert_eq!(arr.get(0), 1);
        assert_eq!(arr.get(4), 0);
        assert_eq!(arr.get(8), 3);
    }

    #[test]
    fn clear_resets_and_returns_previous_onebit() {
        let mut arr = BitArray::<OneBit, 8>::new();

        arr.put(6, true);
        assert_eq!(arr.clear(6), true);
        assert_eq!(arr.get(6), false);
    }

    #[test]
    fn clear_resets_and_returns_previous_twobits() {
        let mut arr = BitArray::<TwoBits, 8>::new();

        arr.put(6, 2);
        assert_eq!(arr.clear(6), 2);
        assert_eq!(arr.get(6), 0);
    }

    #[test]
    fn first_and_last_index_onebit() {
        let mut arr = BitArray::<OneBit, 17>::new();

        arr.put(0, true);
        arr.put(16, true);

        assert_eq!(arr.get(0), true);
        assert_eq!(arr.get(16), true);
    }

    #[test]
    fn first_and_last_index_twobits() {
        let mut arr = BitArray::<TwoBits, 9>::new();

        arr.put(0, 1);
        arr.put(8, 3);

        assert_eq!(arr.get(0), 1);
        assert_eq!(arr.get(8), 3);
    }

    #[test]
    #[should_panic(expected = "Index out of bounds")]
    fn get_panics_on_index_n_onebit() {
        let arr = BitArray::<OneBit, 8>::new();
        let _ = arr.get(8);
    }

    #[test]
    #[should_panic(expected = "Index out of bounds")]
    fn put_panics_on_index_n_onebit() {
        let mut arr = BitArray::<OneBit, 8>::new();
        let _ = arr.put(8, true);
    }

    #[test]
    #[should_panic(expected = "Index out of bounds")]
    fn clear_panics_on_index_n_onebit() {
        let mut arr = BitArray::<OneBit, 8>::new();
        let _ = arr.clear(8);
    }

    #[test]
    #[should_panic(expected = "Index out of bounds")]
    fn get_panics_on_index_n_twobits() {
        let arr = BitArray::<TwoBits, 8>::new();
        let _ = arr.get(8);
    }

    #[test]
    #[should_panic(expected = "Index out of bounds")]
    fn put_panics_on_index_n_twobits() {
        let mut arr = BitArray::<TwoBits, 8>::new();
        let _ = arr.put(8, 1);
    }

    #[test]
    #[should_panic(expected = "Index out of bounds")]
    fn clear_panics_on_index_n_twobits() {
        let mut arr = BitArray::<TwoBits, 8>::new();
        let _ = arr.clear(8);
    }

    #[test]
    fn put_unchecked_matches_put_onebit() {
        let mut arr = BitArray::<OneBit, 9>::new();

        let safe_prev = arr.put(4, true);
        assert_eq!(safe_prev, false);

        let unsafe_prev = unsafe { arr.put_unchecked(4, false) };
        assert_eq!(unsafe_prev, true);
        assert_eq!(arr.get(4), false);
    }

    #[test]
    fn get_unchecked_matches_get_onebit() {
        let mut arr = BitArray::<OneBit, 9>::new();
        arr.put(7, true);

        let safe = arr.get(7);
        let unchecked = unsafe { arr.get_unchecked(7) };

        assert_eq!(safe, unchecked);
    }

    #[test]
    fn put_unchecked_matches_put_twobits() {
        let mut arr = BitArray::<TwoBits, 9>::new();

        let safe_prev = arr.put(5, 2);
        assert_eq!(safe_prev, 0);

        let unsafe_prev = unsafe { arr.put_unchecked(5, 3) };
        assert_eq!(unsafe_prev, 2);
        assert_eq!(arr.get(5), 3);
    }

    #[test]
    fn get_unchecked_matches_get_twobits() {
        let mut arr = BitArray::<TwoBits, 9>::new();
        arr.put(8, 2);

        let safe = arr.get(8);
        let unchecked = unsafe { arr.get_unchecked(8) };

        assert_eq!(safe, unchecked);
    }

    #[test]
    fn full_array_pattern_onebit() {
        let mut arr = BitArray::<OneBit, 33>::new();

        for i in 0..33 {
            arr.put(i, i % 2 == 0);
        }

        for i in 0..33 {
            assert_eq!(arr.get(i), i % 2 == 0);
        }
    }

    #[test]
    fn full_array_pattern_twobits() {
        let mut arr = BitArray::<TwoBits, 33>::new();

        for i in 0..33 {
            arr.put(i, (i % 4) as u8);
        }

        for i in 0..33 {
            assert_eq!(arr.get(i), (i % 4) as u8);
        }
    }

    #[test]
    fn repeated_overwrite_onebit_final_pass_wins() {
        let mut arr = BitArray::<OneBit, 20>::new();

        for i in 0..20 {
            arr.put(i, true);
        }
        for i in 0..20 {
            arr.put(i, false);
        }
        for i in 0..20 {
            arr.put(i, i % 3 == 0);
        }

        for i in 0..20 {
            assert_eq!(arr.get(i), i % 3 == 0);
        }
    }

    #[test]
    fn repeated_overwrite_twobits_final_pass_wins() {
        let mut arr = BitArray::<TwoBits, 20>::new();

        for i in 0..20 {
            arr.put(i, 3);
        }
        for i in 0..20 {
            arr.put(i, 1);
        }
        for i in 0..20 {
            arr.put(i, ((i * 3) % 4) as u8);
        }

        for i in 0..20 {
            assert_eq!(arr.get(i), ((i * 3) % 4) as u8);
        }
    }
}

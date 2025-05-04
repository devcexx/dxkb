use core::ops::Index;

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct BoundedIndex<const LENGTH: usize>(usize);

impl<const LENGTH: usize> BoundedIndex<LENGTH> {
    pub const fn assert_range_ok(value: usize) {
        assert!(value < LENGTH, "Value out of bounds");
    }

    pub const fn from_const<const N: usize>() -> Self {
        const {
            Self::assert_range_ok(N);
        }

        Self(N)
    }

    pub fn from_value(val: usize) -> Option<Self> {
        if val < LENGTH { Some(Self(val)) } else { None }
    }

    #[inline(always)]
    pub fn index(&self) -> usize {
        self.0
    }
}

impl<A, const LENGTH: usize> Index<BoundedIndex<LENGTH>> for [A; LENGTH] {
    type Output = A;

    fn index(&self, index: BoundedIndex<LENGTH>) -> &Self::Output {
        unsafe {
            // SAFETY: index <= slice length asserted at type level.
            self.get_unchecked(index.0)
        }
    }
}

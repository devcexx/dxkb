use core::{
    fmt::{Debug, Display},
    ops::Index,
};

use super::{ConstCond, IsTrue};

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct BoundedIndex<const LENGTH: usize>(usize);

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct BoundedU8<const LENGTH: u8>(u8);

impl<const LENGTH: u8> Display for BoundedU8<LENGTH> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<const LENGTH: u8> Debug for BoundedU8<LENGTH> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "BoundedU8<{}>({})", LENGTH, self.0)
    }
}

impl<const LENGTH: u8> BoundedU8<LENGTH>
where
    ConstCond<{ LENGTH > 0 }>: IsTrue,
{
    pub const ZERO: BoundedU8<LENGTH> = BoundedU8(0);
}

impl<const LENGTH: u8> BoundedU8<LENGTH> {
    pub const fn assert_range_ok(value: u8) {
        assert!(value < LENGTH, "Value out of bounds");
    }

    pub const fn from_const<const N: u8>() -> Self {
        const { Self::assert_range_ok(N) }

        Self(N)
    }

    pub fn from_value(val: u8) -> Option<Self> {
        if val < LENGTH { Some(Self(val)) } else { None }
    }

    pub unsafe fn from_value_unchecked(val: u8) -> Self {
        BoundedU8(val)
    }

    #[inline(always)]
    pub fn index(&self) -> u8 {
        self.0
    }

    #[inline(always)]
    pub fn increment(&self) -> Option<Self> {
        if self.0 >= LENGTH || self.0 == u8::MAX {
            return None;
        }

        unsafe {
            // SAFETY: Next value validity checked before
            return Some(Self::from_value_unchecked(self.0 + 1));
        }
    }
}

impl<A, const LENGTH: u8> Index<BoundedU8<LENGTH>> for [A; LENGTH as usize]
where
    [(); LENGTH as usize]:,
{
    type Output = A;

    fn index(&self, index: BoundedU8<LENGTH>) -> &Self::Output {
        unsafe {
            // SAFETY: index <= slice length asserted at type level.
            self.get_unchecked(index.0 as usize)
        }
    }
}

impl<const LENGTH: usize> BoundedIndex<LENGTH> {
    pub const fn assert_range_ok(value: usize) {
        assert!(value < LENGTH, "Value out of bounds");
    }

    pub const fn from_const<const N: usize>() -> Self {
        const { Self::assert_range_ok(N) }

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

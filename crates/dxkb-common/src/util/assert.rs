/// A dummy struct that may be used in where clauses to run const assertions
/// over a type's generic types.
///
/// Example:
///
/// ```rust
/// const NON_ZERO_ASSERT<const VAL: u8>: () = const {
///     assert!(VAL > 0, "Value cannot be zero");
/// };
/// ```
///
/// And then, when defining a new type, the following can be done:
///
/// ```rust
/// struct NonZeroU8<const N: u8> where Assert<{NON_ZERO_ASSERT::<N>}>:;
/// ```
pub struct Assert<const R: ()> {}

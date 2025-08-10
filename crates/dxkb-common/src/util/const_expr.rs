pub trait IsTrue {}
pub enum ConstCond<const B: bool> {}
impl IsTrue for ConstCond<true> {}

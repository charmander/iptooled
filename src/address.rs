pub const ADDRESS_BYTES: usize = 16;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Address(pub [u8; ADDRESS_BYTES]);

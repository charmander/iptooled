use std::convert::TryInto;

pub const ADDRESS_BYTES: usize = 16;
pub const ADDRESS_BITS: usize = 8 * ADDRESS_BYTES;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Address(pub [u8; ADDRESS_BYTES]);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AddressPrefix {
	/// The first address with this prefix, i.e. the one ending with `ADDRESS_BITS - bits` zero bits.
	first: Address,

	/// The number of bits in the prefix.
	bits: u32,
}

impl Address {
	pub fn prefix(&self, bits: usize) -> AddressPrefix {
		assert!(bits <= ADDRESS_BITS);

		let mut result = [0; ADDRESS_BYTES];
		let wholes = bits / 8;
		let remainder = bits % 8;
		result[..wholes].copy_from_slice(&self.0[..wholes]);

		if remainder != 0 {
			result[wholes + 1] = self.0[wholes + 1] & mask(remainder);
		}

		AddressPrefix {
			first: Address(result),
			bits: bits.try_into().unwrap(),
		}
	}
}

impl AddressPrefix {
	pub fn bytes(&self) -> &[u8] {
		let count = (self.bits + 7) / 8;
		&self.first.0[..count.try_into().unwrap()]
	}

	pub fn bits(&self) -> u32 {
		self.bits
	}

	pub fn first(self) -> Address {
		self.first
	}
}

/// A byte with the first n bits set.
const fn mask(n: usize) -> u8 {
	!(0xff_u8 >> n)
}

pub const ADDRESS_BYTES: usize = 16;
pub const ADDRESS_BITS: u8 = 8 * (ADDRESS_BYTES as u8);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Address(pub [u8; ADDRESS_BYTES]);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AddressPrefix {
	/// The first address with this prefix, i.e. the one ending with `ADDRESS_BITS - bits` zero bits.
	first: Address,

	/// The number of bits in the prefix.
	bits: u8,
}

impl Address {
	pub fn prefix(&self, bits: u8) -> AddressPrefix {
		assert!(bits <= ADDRESS_BITS);

		let mut result = [0; ADDRESS_BYTES];
		let wholes = usize::from(bits / 8);
		let remainder = bits % 8;
		result[..wholes].copy_from_slice(&self.0[..wholes]);

		if remainder != 0 {
			result[wholes + 1] = self.0[wholes + 1] & mask(remainder);
		}

		AddressPrefix {
			first: Address(result),
			bits,
		}
	}
}

impl AddressPrefix {
	pub fn bits(&self) -> u8 {
		self.bits
	}

	/// Shortens the prefix in place by one bit. Panics if it’s empty.
	pub fn shorten(&mut self) {
		self.bits = self.bits.checked_sub(1).expect("tried to shorten an empty AddressPrefix");

		let new_byte = self.bits / 8;
		let new_bit = self.bits % 8;

		if new_bit == 7 {
			self.first.0[usize::from(new_byte + 1)] = 0;
		} else {
			self.first.0[usize::from(new_byte)] &= mask(new_bit);
		}
	}

	pub fn is_prefix_of(&self, address: &Address) -> bool {
		let Self { first, bits } = self;
		let wholes = bits / 8;
		let remainder = bits % 8;

		first.0[..usize::from(wholes)] == address.0[..usize::from(wholes)]
			&& (remainder == 0 || (first.0[usize::from(wholes + 1)] ^ address.0[usize::from(wholes + 1)]) & mask(remainder) == 0)
	}
}

/// A byte with the first n bits set.
const fn mask(n: u8) -> u8 {
	!(0xff_u8 >> n)
}

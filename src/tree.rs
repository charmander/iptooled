pub const ADDRESS_BYTES: usize = 16;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Address(pub [u8; ADDRESS_BYTES]);

#[derive(Clone, Debug)]
pub struct QueryResult {
	pub trusted_count: u32,
	pub spam_count: u32,
	pub prefix_bits: u8,
}

struct AddressPath {
	address: Address,
	path_index: usize,
}

impl AddressPath {
	fn new(address: Address) -> Self {
		Self {
			address,
			path_index: 0,
		}
	}
}

impl Iterator for AddressPath {
	type Item = u8;

	fn next(&mut self) -> Option<Self::Item> {
		let address_index = self.path_index / 2;
		let low = self.path_index % 2 == 1;

		if address_index == self.address.0.len() {
			return None;
		}

		let byte = self.address.0[address_index];

		self.path_index += 1;

		Some(
			if low {
				byte & 0xf
			} else {
				byte >> 4
			}
		)
	}
}

#[derive(Clone, Debug)]
pub struct AddressTree {
	children: [Option<Box<AddressTree>>; 16],
	trusted_count: u32,
	spam_count: u32,
}

impl AddressTree {
	pub fn query(&self, address: Address) -> QueryResult {
		let mut current = self;
		let mut prefix_bits = 0;

		for index in AddressPath::new(address) {
			if let Some(child) = &self.children[usize::from(index)] {
				current = child;
				prefix_bits += 4;
			} else {
				break;
			}
		}

		let Self { trusted_count, spam_count, .. } = *current;

		QueryResult {
			trusted_count,
			spam_count,
			prefix_bits,
		}
	}

	fn mark_path_trusted(&mut self, mut path: impl Iterator<Item = u8>) {
		self.trusted_count += 1;

		if self.spam_count == 0 {
			return;
		}

		if let Some(next) = path.next() {
			if let Some(child) = &mut self.children[usize::from(next)] {
				child.mark_path_trusted(path);
			}
		}
	}

	pub fn mark_trusted(&mut self, address: Address) {
		self.mark_path_trusted(AddressPath::new(address));
	}

	fn mark_path_spam(&mut self, mut path: impl Iterator<Item = u8>) {
		self.spam_count += 1;

		if let Some(next) = path.next() {
			self.children[usize::from(next)].get_or_insert_with(|| {
				Box::new(AddressTree {
					children: Default::default(),
					trusted_count: 0,
					spam_count: 0,
				})
			}).mark_path_spam(path);
		}
	}

	pub fn mark_spam(&mut self, address: Address) {
		self.mark_path_spam(AddressPath::new(address));
	}
}

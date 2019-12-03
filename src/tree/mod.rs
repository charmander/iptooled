mod node_index;

use super::address::Address;
use self::node_index::{AddressPath, NodeIndex, NodeArray};

#[derive(Clone, Debug)]
pub struct QueryResult {
	pub trusted_count: u32,
	pub spam_count: u32,
	pub prefix_bits: u8,
}

#[derive(Clone, Debug)]
pub struct AddressTree {
	children: NodeArray<Option<Box<AddressTree>>>,
	trusted_count: u32,
	spam_count: u32,
}

impl AddressTree {
	pub fn new() -> Self {
		Self {
			children: Default::default(),
			trusted_count: 0,
			spam_count: 0,
		}
	}

	pub fn query(&self, address: Address) -> QueryResult {
		let mut current = self;
		let mut prefix_bits = 0;

		for index in AddressPath::new(address) {
			if let Some(child) = &self.children[index] {
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

	fn record_trusted_path(&mut self, mut path: impl Iterator<Item = NodeIndex>) {
		self.trusted_count += 1;

		if self.spam_count == 0 {
			return;
		}

		if let Some(next) = path.next() {
			if let Some(child) = &mut self.children[next] {
				child.record_trusted_path(path);
			}
		}
	}

	pub fn record_trusted(&mut self, address: Address) {
		self.record_trusted_path(AddressPath::new(address))
	}

	fn record_spam_path(&mut self, mut path: impl Iterator<Item = NodeIndex>) {
		self.spam_count += 1;

		if let Some(next) = path.next() {
			self.children[next].get_or_insert_with(|| {
				Box::new(AddressTree::new())
			}).record_spam_path(path);
		}
	}

	pub fn record_spam(&mut self, address: Address) {
		self.record_spam_path(AddressPath::new(address));
	}
}

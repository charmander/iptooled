mod node_index;

use siphasher::sip::SipHasher24;
use std::convert::TryInto;
use std::hash::Hasher;

use super::address::{ADDRESS_BYTES, Address, AddressPrefix};
use self::node_index::{AddressPath, NodeIndex, NodeArray};

/// The minimum number of bits of prefix to record for a trusted address. Can’t be zero. Should be a multiple of the number of bits in an index.
const MINIMUM_BITS: usize = 20;

#[derive(Clone, Debug)]
pub struct QueryResult {
	pub trusted_count: u32,
	pub spam_count: u32,
	pub prefix_bits: u8,
}

#[derive(Clone, Debug)]
#[must_use]
pub enum TreeOperation {
	Trust(AddressPrefix),
	Spam(Address),
}

#[derive(Clone, Debug)]
pub struct SerializedTreeOperation {
	pub bytes: [u8; 1 + ADDRESS_BYTES + 8],
}

fn serialize(hasher: &mut SipHasher24, operation: TreeOperation) -> SerializedTreeOperation {
	let mut bytes = [0; 1 + ADDRESS_BYTES + 8];
	let _size = operation.serialize((&mut bytes[..1 + ADDRESS_BYTES]).try_into().unwrap());
	hasher.write(&bytes[..1 + ADDRESS_BYTES]);
	bytes[1 + ADDRESS_BYTES..].copy_from_slice(&hasher.finish().to_be_bytes());
	SerializedTreeOperation { bytes }
}

impl TreeOperation {
	fn serialize(&self, buf: &mut [u8; 1 + ADDRESS_BYTES]) -> usize {
		match self {
			Self::Trust(prefix) => {
				let bits = prefix.bits();

				assert!(bits != 0);

				let prefix_bytes = prefix.bytes();
				buf[0] = bits.try_into().unwrap();
				buf[1..(1 + prefix_bytes.len())].copy_from_slice(prefix_bytes);
				1 + prefix_bytes.len()
			}
			Self::Spam(address) => {
				buf[0] = 0;
				buf[1..(1 + ADDRESS_BYTES)].copy_from_slice(&address.0);
				1 + ADDRESS_BYTES
			}
		}
	}

	pub fn deserialize(buf: &[u8; 1 + ADDRESS_BYTES]) -> Self {
		let address = Address(buf[1..].try_into().unwrap());

		match buf[0] {
			0 => Self::Spam(address),
			bits => Self::Trust(address.prefix(bits.into())),
		}
	}

	pub fn apply(self, tree: &mut AddressTree) -> SerializedTreeOperation {
		match self {
			Self::Trust(prefix) => tree.record_trusted(prefix.first()),
			Self::Spam(address) => tree.record_spam(address),
		}
	}
}

#[derive(Clone, Debug)]
struct AddressTreeNode {
	children: NodeArray<Option<Box<AddressTreeNode>>>,
	trusted_count: u32,
	spam_count: u32,
}

impl AddressTreeNode {
	fn new() -> Self {
		Self {
			children: Default::default(),
			trusted_count: 0,
			spam_count: 0,
		}
	}

	fn get_or_create_child(&mut self, index: NodeIndex) -> &mut Self {
		self.children[index]
			.get_or_insert_with(|| Box::new(AddressTreeNode::new()))
	}
}

#[derive(Clone, Debug)]
pub struct AddressTree {
	root: AddressTreeNode,

	// It feels a bit out of place, but I’m not sure where else to put it without overcomplicating the code.
	checksum: SipHasher24,
}

impl AddressTree {
	pub fn new_with_keys(key0: u64, key1: u64) -> Self {
		Self {
			root: AddressTreeNode::new(),
			checksum: SipHasher24::new_with_keys(key0, key1),
		}
	}

	pub fn query(&self, address: &Address) -> QueryResult {
		let mut current = &self.root;
		let mut prefix_bits = 0;

		for index in AddressPath::new(address) {
			if let Some(child) = &current.children[index] {
				current = child;
				prefix_bits += 4;
			} else {
				break;
			}
		}

		let AddressTreeNode { trusted_count, spam_count, .. } = *current;

		QueryResult {
			trusted_count,
			spam_count,
			prefix_bits,
		}
	}

	pub fn record_trusted(&mut self, address: Address) -> SerializedTreeOperation {
		let mut current = &mut self.root;
		let mut prefix_bits = 0;
		let mut path = AddressPath::new(&address);

		loop {
			current.trusted_count = current.trusted_count.saturating_add(1);

			if prefix_bits >= MINIMUM_BITS && current.spam_count == 0 {
				break;
			}

			if let Some(index) = path.next() {
				current = current.get_or_create_child(index);
				prefix_bits += 4;
			} else {
				break;
			}
		}

		serialize(&mut self.checksum, TreeOperation::Trust(address.prefix(prefix_bits)))
	}

	pub fn record_spam(&mut self, address: Address) -> SerializedTreeOperation {
		let mut current = &mut self.root;
		let mut path = AddressPath::new(&address);

		loop {
			current.spam_count = current.spam_count.saturating_add(1);

			if let Some(index) = path.next() {
				current = current.get_or_create_child(index);
			} else {
				break;
			}
		}

		serialize(&mut self.checksum, TreeOperation::Spam(address))
	}
}

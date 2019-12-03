use std::ops::{Index, IndexMut};

use super::super::address::Address;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeIndex(u8);

impl NodeIndex {
	const fn high(byte: u8) -> Self {
		Self(byte >> 4)
	}

	const fn low(byte: u8) -> Self {
		Self(byte & 0xf)
	}
}

/// An array of nodes that can be indexed exactly by a `NodeIndex`.
#[derive(Clone, Debug, Default)]
pub struct NodeArray<T>([T; 16]);

impl<T> Index<NodeIndex> for NodeArray<T> {
	type Output = T;

	fn index(&self, key: NodeIndex) -> &T {
		&self.0[usize::from(key.0)]
	}
}

impl<T> IndexMut<NodeIndex> for NodeArray<T> {
	fn index_mut(&mut self, key: NodeIndex) -> &mut T {
		&mut self.0[usize::from(key.0)]
	}
}

/// An iterator over the `NodeIndex`es of the path to an address.
pub struct AddressPath {
	address: Address,
	path_index: usize,
}

impl AddressPath {
	pub fn new(address: Address) -> Self {
		Self {
			address,
			path_index: 0,
		}
	}
}

impl Iterator for AddressPath {
	type Item = NodeIndex;

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
				NodeIndex::low(byte)
			} else {
				NodeIndex::high(byte)
			}
		)
	}
}

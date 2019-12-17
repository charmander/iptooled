use std::collections::{btree_map, hash_map, BTreeMap, HashMap};

use super::address::{ADDRESS_BITS, Address, AddressPrefix};
use super::time_list::{CoarseDuration, CoarseSystemTime, TimeList};

const ENTRIES_PER_USER: u8 = 5;

/// The smallest shared prefix size considered meaningful. For IPv6, at least 4, because the entire internet is in 2000::/3.
const PREFIX_BITS_MINIMUM: u8 = 12;

/// The time before an entry’s user information is discarded, making the effective number of entries per user `ENTRIES_PER_USER * ADDRESS_EXPIRY_HOURS / USER_EXPIRY_HOURS`.
const USER_EXPIRY_HOURS: CoarseDuration = CoarseDuration { hours: 24 * 30 };

/// The time before an entry stops being considered useful and is discarded.
const ADDRESS_EXPIRY_HOURS: CoarseDuration = CoarseDuration { hours: 24 * 365 * 2 };

pub const USER_BYTES: usize = 4;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct User(u32);

impl User {
	pub const fn from_bytes(bytes: [u8; USER_BYTES]) -> Self {
		Self(u32::from_be_bytes(bytes))
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpamStats {
	pub trusted_users: u32,
	pub spam_users: u32,
}

impl SpamStats {
	pub const EMPTY: Self = Self {
		trusted_users: 0,
		spam_users: 0,
	};
}

#[derive(Clone, Debug)]
pub struct QueryResult {
	pub stats: SpamStats,
	pub prefix_bits: u8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum OperationType {
	Trust,
	Spam,
}

#[derive(Clone, Debug)]
pub struct Operation(OperationType, Address, User);

#[derive(Clone, Debug)]
struct AddressOperation(OperationType, Address);

#[derive(Clone, Debug)]
pub struct SpamTree {
	users: HashMap<User, u8>,
	counts: BTreeMap<AddressPrefix, SpamStats>,
	user_window: TimeList<Operation>,
	address_window: TimeList<AddressOperation>,
}

impl SpamTree {
	pub fn new() -> Self {
		Self {
			users: HashMap::new(),
			counts: BTreeMap::new(),
			user_window: TimeList::new(USER_EXPIRY_HOURS),
			address_window: TimeList::new(ADDRESS_EXPIRY_HOURS),
		}
	}

	pub fn query_stale(&self, address: &Address) -> QueryResult {
		let mut prefix = address.prefix(ADDRESS_BITS);

		loop {
			let (key, value) =
				match self.counts.range(..=&prefix).next_back() {
					Some(pair) => pair,
					None => break,
				};

			if key.bits() <= prefix.bits() && key.is_prefix_of(&address) {
				return QueryResult {
					stats: value.clone(),
					prefix_bits: key.bits(),
				};
			}

			if prefix.bits() == PREFIX_BITS_MINIMUM {
				break;
			}

			// TODO: shorten to `key` bits − 1?
			prefix.shorten();
		}

		QueryResult {
			stats: SpamStats::EMPTY,
			prefix_bits: 0,
		}
	}

	fn advance(&mut self, now: CoarseSystemTime) {
		for (Operation(type_, address, user), time) in self.user_window.trim(now) {
			let entry = match self.users.entry(user) {
				hash_map::Entry::Occupied(o) => o,
				hash_map::Entry::Vacant(_) => panic!("User unexpectedly missing from map"),
			};

			if *entry.get() > 1 {
				*entry.into_mut() -= 1;
			} else {
				entry.remove();
			}

			self.address_window.push(AddressOperation(type_, address), time);
		}

		for (AddressOperation(type_, address), _time) in self.address_window.trim(now) {
			Self::unapply(&mut self.counts, &address, match type_ {
				OperationType::Trust => |entry| {
					entry.trusted_users -= 1;
				},
				OperationType::Spam => |entry| {
					entry.spam_users -= 1;
				},
			});
		}
	}

	pub fn query(&mut self, address: &Address, now: CoarseSystemTime) -> QueryResult {
		self.advance(now);
		self.query_stale(&address)
	}

	fn try_increment(&mut self, user: User) -> Option<()> {
		// Limit the number of entries stored for one user.
		match self.users.entry(user) {
			hash_map::Entry::Occupied(entry) => {
				let count = entry.into_mut();

				if *count == ENTRIES_PER_USER {
					return None;
				}

				*count += 1;
			}
			hash_map::Entry::Vacant(entry) => {
				entry.insert(1);
			}
		}

		Some(())
	}

	fn apply(counts: &mut BTreeMap<AddressPrefix, SpamStats>, address: &Address, entry_update: impl Fn(btree_map::Entry<AddressPrefix, SpamStats>) -> ()) {
		let mut prefix = address.prefix(ADDRESS_BITS);

		loop {
			entry_update(counts.entry(prefix.clone()));

			if prefix.bits() == PREFIX_BITS_MINIMUM {
				break;
			}

			prefix.shorten();
		}
	}

	fn unapply(counts: &mut BTreeMap<AddressPrefix, SpamStats>, address: &Address, entry_update: fn(&mut SpamStats) -> ()) {
		Self::apply(counts, address, |entry| {
			let mut entry = match entry {
				btree_map::Entry::Occupied(entry) => entry,
				btree_map::Entry::Vacant(_) => panic!("Address unexpectedly missing from map"),
			};

			entry_update(entry.get_mut());

			if entry.get() == &SpamStats::EMPTY {
				entry.remove();
			}
		});
	}

	pub fn trust(&mut self, address: Address, user: User, now: CoarseSystemTime) {
		self.advance(now);

		if self.try_increment(user).is_none() {
			return;
		}

		Self::apply(&mut self.counts, &address, |entry| {
			entry
				.or_insert(SpamStats::EMPTY)
				.trusted_users += 1;
		});

		self.user_window.push(Operation(OperationType::Trust, address, user), now);
	}

	pub fn spam(&mut self, address: Address, user: User, now: CoarseSystemTime) {
		self.advance(now);

		if self.try_increment(user).is_none() {
			return;
		}

		Self::apply(&mut self.counts, &address, |entry| {
			entry
				.or_insert(SpamStats::EMPTY)
				.spam_users += 1;
		});

		self.user_window.push(Operation(OperationType::Spam, address, user), now);
	}
}

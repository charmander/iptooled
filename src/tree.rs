use std::collections::{hash_map, BTreeMap, HashMap, VecDeque};
use std::convert::TryFrom;
use std::time;

use super::address::{ADDRESS_BITS, Address, AddressPrefix};

const ENTRIES_PER_USER: u8 = 5;

/// The smallest shared prefix size considered meaningful. For IPv6, at least 4, because the entire internet is in 2000::/3.
const PREFIX_BITS_MINIMUM: u8 = 12;

/// The time before an entry stops being considered useful and is discarded.
const EXPIRY_HOURS: CoarseDuration = CoarseDuration { hours: 24 * 365 * 2 };

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct User(u32);

impl User {
	pub const fn from_bytes(bytes: [u8; 4]) -> Self {
		Self(u32::from_be_bytes(bytes))
	}
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CoarseDuration {
	hours: u16,  // 2^16 hours is 7.5 years
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
enum TimeError {
	Overflow(time::Duration),
	LongJumpBackwards(time::SystemTimeError),
}

#[derive(Clone, Debug)]
pub enum Operation {
	Trust(Address, User),
	Spam(Address, User),
}

#[derive(Clone, Debug)]
pub struct SpamTree {
	users: HashMap<User, u8>,
	counts: BTreeMap<AddressPrefix, SpamStats>,
	window: VecDeque<Operation>,
	time_reference: time::SystemTime,
}

impl SpamTree {
	pub fn new(time_reference: time::SystemTime) -> Self {
		Self {
			users: HashMap::new(),
			counts: BTreeMap::new(),
			window: VecDeque::new(),
			time_reference,
		}
	}

	fn translate_time(&self, time: time::SystemTime) -> Result<CoarseDuration, TimeError> {
		match time.duration_since(self.time_reference) {
			Ok(duration) =>
				match u16::try_from(duration.as_secs() / 3600) {
					Ok(hours) => Ok(CoarseDuration { hours }),
					Err(_) => Err(TimeError::Overflow(duration)),
				},
			Err(err) if err.duration() < time::Duration::from_secs(3600) => Ok(CoarseDuration { hours: 0 }),
			Err(err) => Err(TimeError::LongJumpBackwards(err)),
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

	fn advance(&mut self, now: CoarseDuration) {
		unimplemented!()
	}

	pub fn query(&mut self, address: &Address, now: time::SystemTime) -> QueryResult {
		let now = self.translate_time(now).unwrap();
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

	pub fn trust(&mut self, address: Address, user: User, now: time::SystemTime) -> Option<&Operation> {
		let now = self.translate_time(now).unwrap();
		self.advance(now);

		self.try_increment(user)?;

		let mut prefix = address.prefix(ADDRESS_BITS);

		loop {
			self.counts.entry(prefix.clone())
				.or_insert(SpamStats::EMPTY)
				.trusted_users += 1;

			if prefix.bits() == PREFIX_BITS_MINIMUM {
				break;
			}

			prefix.shorten();
		}

		self.window.push_back(Operation::Trust(address, user));
		Some(self.window.back().unwrap())
	}

	pub fn spam(&mut self, address: Address, user: User, now: time::SystemTime) -> Option<&Operation> {
		unimplemented!();
	}
}

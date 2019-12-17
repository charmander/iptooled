use std::collections::VecDeque;
use std::convert::TryFrom;
use std::ops::{AddAssign, Sub};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CoarseDuration {
	pub hours: u16,  // 2^16 hours is 7.5 years
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CoarseSystemTime {
	epoch_hours: u32,
}

impl CoarseSystemTime {
	/// Gets the current time with a precision of one hour.
	pub fn now() -> Self {
		let epoch_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("SystemTime before Unix epoch");

		Self {
			epoch_hours: u32::try_from(epoch_time.as_secs() / 3600).expect("SystemTime after year 491936"),
		}
	}

	/// Gets the time since a reference time, returning zero for times up to one hour later, panicking for times later than that, and panicking for times 2^16 or more hours earlier.
	pub fn time_since(self, other: Self) -> CoarseDuration {
		let hours =
			if self.epoch_hours < other.epoch_hours {
				if self.epoch_hours + 1 < other.epoch_hours {
					panic!("Tried to get the time since a time more than an hour in the future");
				}

				0
			} else {
				u16::try_from(self.epoch_hours - other.epoch_hours)
					.expect("Tried to get the time since 2^16 or more hours in the past")
			};

		CoarseDuration { hours }
	}
}

impl AddAssign<CoarseDuration> for CoarseSystemTime {
	fn add_assign(&mut self, duration: CoarseDuration) {
		self.epoch_hours = self.epoch_hours.checked_add(duration.hours.into())
			.expect("Addition resulted in a time after year 491936");
	}
}

impl Sub<CoarseDuration> for CoarseSystemTime {
	type Output = Self;

	fn sub(self, duration: CoarseDuration) -> Self {
		let epoch_hours = self.epoch_hours.checked_sub(duration.hours.into())
			.expect("Subtraction resulted in a time before Unix epoch");

		Self { epoch_hours }
	}
}

#[derive(Clone, Debug)]
struct Entry<T> {
	value: T,
	offset: CoarseDuration,
}

#[derive(Clone, Debug)]
pub struct TimeList<T> {
	values: VecDeque<Entry<T>>,
	head_tail: Option<(CoarseSystemTime, CoarseSystemTime)>,
	limit: CoarseDuration,
}

impl<T> TimeList<T> {
	pub fn new(limit: CoarseDuration) -> Self {
		Self {
			values: VecDeque::new(),
			head_tail: None,
			limit,
		}
	}

	/// Adds a value to the end of the list, associated with a time. Doesn’t trim the list, so the time doesn’t have to be the current time, but it does have to be at least as late as the other times in the list.
	pub fn push(&mut self, value: T, time: CoarseSystemTime) {
		let offset =
			match self.head_tail {
				None => {
					self.head_tail = Some((time, time));
					CoarseDuration { hours: 0 }
				},
				Some((_, ref mut tail)) => {
					let offset = time.time_since(*tail);
					*tail = time;
					offset
				},
			};

		self.values.push_back(Entry {
			value,
			offset,
		});
	}

	pub fn trim<'a>(&'a mut self, now: CoarseSystemTime) -> Trim<'a, T> {
		let cutoff = now - self.limit;

		Trim {
			list: self,
			cutoff,
		}
	}
}

pub struct Trim<'a, T> {
	list: &'a mut TimeList<T>,
	cutoff: CoarseSystemTime,
}

impl<'a, T> Iterator for Trim<'a, T> {
	type Item = (T, CoarseSystemTime);

	fn next(&mut self) -> Option<Self::Item> {
		let (ref mut head, _) = self.list.head_tail?;
		let trim_time = *head;

		if trim_time >= self.cutoff {
			return None;
		}

		let trimmed = self.list.values.pop_front().unwrap();
		debug_assert!(trimmed.offset == CoarseDuration { hours: 0 });

		if let Some(next) = self.list.values.front_mut() {
			*head += next.offset;
			next.offset = CoarseDuration { hours: 0 };
		} else {
			self.list.head_tail = None;
		}

		Some((trimmed.value, trim_time))
	}
}

use quickcheck::{Arbitrary, Gen};
use rand::Rng;

use super::{CoarseDuration, CoarseSystemTime, TimeList};

impl Arbitrary for CoarseDuration {
	fn arbitrary<G: Gen>(g: &mut G) -> Self {
		Self {
			hours: Arbitrary::arbitrary(g),
		}
	}
}

impl Arbitrary for CoarseSystemTime {
	fn arbitrary<G: Gen>(g: &mut G) -> Self {
		Self {
			epoch_hours: g.gen_range(262000, 6400000),  // ~2000 to ~2700
		}
	}
}

impl<T: Arbitrary> Arbitrary for TimeList<T> {
	fn arbitrary<G: Gen>(g: &mut G) -> Self {
		let size = {
			let s = g.size();
			g.gen_range(0, s)
		};

		let mut result = TimeList::new(Arbitrary::arbitrary(g));
		let mut now: CoarseSystemTime = Arbitrary::arbitrary(g);

		for _ in 0..size {
			for _ in result.trim(now) {}
			result.push(Arbitrary::arbitrary(g), now);
			now += Arbitrary::arbitrary(g);
		}

		result
	}
}

#[quickcheck]
fn front_time_is_head(list: TimeList<u32>) -> bool {
	list.values.front().map(|entry| entry.offset)
		== list.head_tail.map(|_| CoarseDuration { hours: 0 })
}

#[quickcheck]
fn back_time_is_tail(list: TimeList<u32>) -> bool {
	let (head, tail) = match list.head_tail {
		Some(t) => t,
		None => return list.values.is_empty(),
	};

	if list.values.is_empty() {
		return false;
	}

	list.values.iter().fold(head, |mut m, n| {
		m += n.offset;
		m
	}) == tail
}

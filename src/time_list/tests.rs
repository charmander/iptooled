use quickcheck::{Arbitrary, Gen};
use rand::Rng;
use rand_distr::Exp1;
use std::convert::TryFrom;

use super::{CoarseDuration, CoarseSystemTime, TimeList};

impl Arbitrary for CoarseDuration {
	fn arbitrary<G: Gen>(g: &mut G) -> Self {
		Self { hours: Arbitrary::arbitrary(g) }
	}
}

#[derive(Clone, Debug)]
struct CoarseGap {
	duration: CoarseDuration,
}

/// A gap between random events with an expected value of one hour.
impl Arbitrary for CoarseGap {
	fn arbitrary<G: Gen>(g: &mut G) -> Self {
		let hours: f32 = g.sample(Exp1);

		// undefined behaviour with probability exp(−2^15), so probably for no actual f32 that Exp1 can produce
		// https://github.com/rust-lang/rust/issues/10184
		let hours = hours as u16;

		Self {
			duration: CoarseDuration { hours },
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

		let mut result = TimeList::new(CoarseDuration {
			hours: g.gen_range(0, u16::try_from(g.size()).unwrap_or(std::u16::MAX)),
		});
		let mut now: CoarseSystemTime = Arbitrary::arbitrary(g);

		for _ in 0..size {
			for _ in result.trim(now) {}
			result.push(Arbitrary::arbitrary(g), now);
			now += (Arbitrary::arbitrary(g): CoarseGap).duration;
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

/// Checks that trimmed values are expired and that untrimmed values are unexpired.
#[quickcheck]
fn trimmed_values_are_expired(mut list: TimeList<u32>, step: CoarseGap) -> bool {
	let limit = list.limit;
	let mut now = match list.head_tail {
		Some((_, tail)) => tail,
		None => return true,
	};

	now += step.duration;

	let expired_correct = list.trim(now).all(|(_, mut time)| {
		time += limit;
		time < now
	});

	let unexpired_correct = match list.head_tail {
		Some((mut head, _)) => {
			head += limit;
			head >= now
		},
		None => true,
	};

	expired_correct && unexpired_correct
}

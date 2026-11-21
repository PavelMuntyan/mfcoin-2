pub const BASE_UNIT: u128 = 1_000_000_000_000;
pub const BLOCKS_PER_YEAR: u32 = 5_256_000;
pub const FIRST_YEAR_EMISSION: u128 = 10_000_000 * BASE_UNIT;
pub const TAIL_EMISSION: u128 = 500_000 * BASE_UNIT;
pub const DECAY_YEARS: u32 = 20;

pub fn emission_for_year(year: u32) -> u128 {
	if year >= DECAY_YEARS {
		return TAIL_EMISSION;
	}
	FIRST_YEAR_EMISSION
		.checked_mul(9u128.pow(year))
		.expect("yearly emission fits u128")
		.checked_div(10u128.pow(year))
		.expect("nonzero denominator")
}

pub fn block_subsidy(year: u32) -> u128 {
	emission_for_year(year) / u128::from(BLOCKS_PER_YEAR)
}

fn percent(value: u128, percent: u128) -> u128 {
	(value / 100) * percent + (value % 100) * percent / 100
}

pub fn split_subsidy(subsidy: u128) -> (u128, u128, u128) {
	let pow = percent(subsidy, 45);
	let grandpa = percent(subsidy, 35);
	let treasury = subsidy - pow - grandpa;
	(pow, grandpa, treasury)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn emission_schedule_and_tail_are_exact() {
		assert_eq!(emission_for_year(0), 10_000_000 * BASE_UNIT);
		assert_eq!(emission_for_year(1), 9_000_000 * BASE_UNIT);
		assert_eq!(emission_for_year(19), 1_350_851_717_672_992_089);
		assert_eq!(block_subsidy(0), 1_902_587_519_025);
		assert_eq!(block_subsidy(20), 95_129_375_951);
		assert_eq!(emission_for_year(20), TAIL_EMISSION);
		assert_eq!(emission_for_year(200), TAIL_EMISSION);
		let total = (0..DECAY_YEARS).map(emission_for_year).sum::<u128>();
		assert_eq!(total, 87_842_334_540_943_071_199);
	}

	#[test]
	fn shares_preserve_the_whole_subsidy() {
		for subsidy in [0, 1, 2, 100, block_subsidy(0), block_subsidy(19), u128::MAX] {
			let (pow, grandpa, treasury) = split_subsidy(subsidy);
			assert_eq!(pow + grandpa + treasury, subsidy);
			assert_eq!(pow, percent(subsidy, 45));
			assert_eq!(grandpa, percent(subsidy, 35));
		}
		assert_eq!(split_subsidy(1), (0, 0, 1));
	}
}

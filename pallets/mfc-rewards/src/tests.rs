use crate::{economics::*, mock::*, *};
use frame_support::traits::Hooks;
use sp_runtime::generic::DigestItem;

fn finalize(block: u64, payout: u64) {
	System::initialize(&block, &Default::default(), &Default::default());
	System::deposit_log(DigestItem::PreRuntime(POW_ENGINE_ID, payout_digest(payout)));
	MfcRewards::on_finalize(block);
}

fn payout_digest(payout: u64) -> Vec<u8> {
	let mut data = vec![PAYOUT_VERSION];
	data.extend_from_slice(account(payout).as_ref());
	data
}

#[test]
fn pow_and_treasury_mature_without_minting_validator_pool() {
	new_test_ext(true).execute_with(|| {
		let subsidy = block_subsidy(0);
		let (pow, validator, treasury) = split_subsidy(subsidy);
		finalize(1, 42);
		assert_eq!(ScheduledTotal::<Test>::get(), subsidy as u64);
		assert_eq!(MintedTotal::<Test>::get(), 0);
		assert_eq!(ValidatorAccrued::<Test>::get(), validator as u64);
		assert_eq!(Balances::free_balance(account(42)), 0);
		finalize(2, 42);
		assert_eq!(MintedTotal::<Test>::get(), 0);
		finalize(3, 42);
		assert_eq!(Balances::free_balance(account(42)), pow as u64);
		assert_eq!(Balances::free_balance(account(99)), treasury as u64);
		assert_eq!(MintedTotal::<Test>::get(), (pow + treasury) as u64);
		assert_eq!(PowPaid::<Test>::get(), pow as u64);
		assert_eq!(TreasuryPaid::<Test>::get(), treasury as u64);
		assert_eq!(ValidatorAccrued::<Test>::get(), (validator * 3) as u64);
		assert_eq!(Balances::total_issuance(), (pow + treasury) as u64);
		assert!(Obligations::<Test>::get(1).is_none());
		assert_eq!(Obligations::<Test>::get(2).unwrap().pow_payout, account(42));
	});
}

#[test]
fn disabled_rewards_do_not_require_a_digest_or_issue_anything() {
	new_test_ext(false).execute_with(|| {
		System::set_block_number(100);
		MfcRewards::on_finalize(100);
		assert_eq!(ScheduledTotal::<Test>::get(), 0);
		assert_eq!(MintedTotal::<Test>::get(), 0);
		assert_eq!(Balances::total_issuance(), 0);
	});
}

#[test]
#[should_panic(expected = "PoW payout digest is required")]
fn missing_payout_digest_is_rejected() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);
		MfcRewards::on_finalize(1);
	});
}

#[test]
#[should_panic(expected = "multiple PoW payout digests")]
fn duplicate_payout_digest_is_rejected() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);
		System::deposit_log(DigestItem::PreRuntime(POW_ENGINE_ID, payout_digest(42)));
		System::deposit_log(DigestItem::PreRuntime(POW_ENGINE_ID, payout_digest(42)));
		MfcRewards::on_finalize(1);
	});
}

#[test]
#[should_panic(expected = "invalid PoW payout digest")]
fn malformed_payout_digest_is_rejected() {
	new_test_ext(true).execute_with(|| {
		System::set_block_number(1);
		System::deposit_log(DigestItem::PreRuntime(POW_ENGINE_ID, vec![1, 2, 3]));
		MfcRewards::on_finalize(1);
	});
}

#[test]
fn payout_uses_canonical_block_state_only() {
	new_test_ext(true).execute_with(|| {
		finalize(1, 10);
		finalize(2, 20);
		assert_eq!(Obligations::<Test>::get(1).unwrap().pow_payout, account(10));
		assert_eq!(Obligations::<Test>::get(2).unwrap().pow_payout, account(20));
		finalize(3, 30);
		assert_eq!(
			Balances::free_balance(account(10)),
			split_subsidy(block_subsidy(0)).0 as u64
		);
		assert_eq!(Balances::free_balance(account(20)), 0);
		assert_eq!(Balances::free_balance(account(30)), 0);
	});
}

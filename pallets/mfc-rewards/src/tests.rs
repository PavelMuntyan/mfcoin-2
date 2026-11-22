use crate::{economics::*, mock::*, *};
use codec::{DecodeAll, Encode};
use finality_grandpa::{Commit, Message, Precommit, SignedPrecommit};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use sp_consensus_grandpa::{
	localized_payload, AuthorityId, AuthoritySignature, GrandpaJustification,
};
use sp_core::{Pair, H256};
use sp_keyring::Ed25519Keyring;
use sp_runtime::{generic::DigestItem, DispatchResult};

fn target_hash(block: u64) -> H256 {
	H256::from_low_u64_be(block)
}

fn finalize(block: u64, payout: u64) {
	let parent = if block == 0 { H256::default() } else { target_hash(block - 1) };
	System::initialize(&block, &parent, &Default::default());
	frame_system::BlockHash::<Test>::insert(block, target_hash(block));
	System::deposit_log(DigestItem::PreRuntime(POW_ENGINE_ID, payout_digest(payout)));
	Grandpa::on_finalize(block);
	MfcRewards::on_finalize(block);
}

fn payout_digest(payout: u64) -> Vec<u8> {
	let mut data = vec![PAYOUT_VERSION];
	data.extend_from_slice(account(payout).as_ref());
	data
}

fn authority(key: Ed25519Keyring) -> AuthorityId {
	key.public().into()
}

fn assert_emission_invariant() {
	assert_eq!(
		MintedTotal::<Test>::get(),
		PowPaid::<Test>::get() + TreasuryPaid::<Test>::get() + ValidatorPaid::<Test>::get()
	);
}

fn justification(target: u64, round: u64, signers: &[Ed25519Keyring]) -> Vec<u8> {
	justification_for_set(target, round, 0, signers)
}

fn justification_for_set(
	target: u64,
	round: u64,
	set_id: u64,
	signers: &[Ed25519Keyring],
) -> Vec<u8> {
	let hash = target_hash(target);
	let precommit = Precommit::new(hash, target);
	let message = Message::Precommit(precommit.clone());
	let precommits = signers
		.iter()
		.map(|key| {
			let payload = localized_payload(round, set_id, &message);
			SignedPrecommit {
				precommit: precommit.clone(),
				signature: key.pair().sign(&payload).into(),
				id: authority(*key),
			}
		})
		.collect();
	let proof: GrandpaJustification<HeaderOf<Test>> = GrandpaJustification {
		round,
		commit: Commit {
			target_hash: hash,
			target_number: target,
			precommits,
		},
		votes_ancestries: vec![],
	};
	proof.encode()
}

fn release(target: u64, proof: Vec<u8>) -> DispatchResult {
	let decoded = GrandpaJustification::<HeaderOf<Test>>::decode_all(&mut &proof[..]).unwrap();
	let signers = decoded.commit.precommits.len() as u32;
	let headers = decoded.votes_ancestries.len() as u32;
	MfcRewards::release_validator_reward(
		RuntimeOrigin::signed(account(1)),
		target,
		proof,
		signers,
		headers,
	)
}

fn register(key: Ed25519Keyring, payout: u64, nonce: u64) -> DispatchResult {
	let id = authority(key);
	let payout = account(payout);
	let payload = validator_payout_payload(System::block_hash(0), 0, &id, &payout, nonce);
	let signature: AuthoritySignature = key.pair().sign(&payload).into();
	MfcRewards::register_validator_payout(
		RuntimeOrigin::signed(account(1)),
		0,
		id,
		payout,
		nonce,
		signature,
	)
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
		assert_eq!(ValidatorPaid::<Test>::get(), 0);
		assert_eq!(Balances::total_issuance(), (pow + treasury) as u64);
		assert!(Obligations::<Test>::get(1).is_none());
		assert_eq!(Obligations::<Test>::get(2).unwrap().pow_payout, account(42));
		assert_eq!(ValidatorObligations::<Test>::get(1).unwrap().amount, validator as u64);
	});
}

#[test]
fn validator_release_pays_actual_signers_once() {
	new_test_ext(true).execute_with(|| {
		finalize(1, 42);
		assert_ok!(register(Ed25519Keyring::Alice, 42, 0));
		assert_ok!(register(Ed25519Keyring::Bob, 43, 0));
		finalize(2, 42);
		finalize(3, 42);
		let validator = split_subsidy(block_subsidy(0)).1 as u64;
		let minted_before = MintedTotal::<Test>::get();
		assert_ok!(release(1, justification(1, 7, &[Ed25519Keyring::Alice, Ed25519Keyring::Bob])));
		assert_eq!(ValidatorPaid::<Test>::get(), validator);
		assert_eq!(ValidatorCreditTotal::<Test>::get(), 0);
		assert_eq!(MintedTotal::<Test>::get(), minted_before + validator);
		assert_eq!(Balances::free_balance(account(42)) - split_subsidy(block_subsidy(0)).0 as u64, validator / 2);
		assert_eq!(Balances::free_balance(account(43)), validator - validator / 2);
		let release_info = ValidatorReleases::<Test>::get(1).unwrap();
		assert_eq!(release_info.round, 7);
		assert_eq!(release_info.signers, 2);
		assert_noop!(
			MfcRewards::release_validator_reward(
				RuntimeOrigin::signed(account(2)),
				1,
				justification(1, 7, &[Ed25519Keyring::Alice, Ed25519Keyring::Bob]),
				2,
				0
			),
			Error::<Test>::AlreadyReleased
		);
		assert_emission_invariant();
	});
}

#[test]
fn unregistered_signer_receives_credit_without_blocking_others() {
	new_test_ext(true).execute_with(|| {
		finalize(1, 42);
		finalize(2, 42);
		finalize(3, 42);
		let validator = split_subsidy(block_subsidy(0)).1 as u64;
		let issuance_before = Balances::total_issuance();
		assert_ok!(release(1, justification(1, 7, &[Ed25519Keyring::Alice, Ed25519Keyring::Bob])));
		assert_eq!(ValidatorPaid::<Test>::get(), 0);
		assert_eq!(ValidatorCreditTotal::<Test>::get(), validator);
		assert_eq!(Balances::total_issuance(), issuance_before);
		assert_noop!(
			MfcRewards::claim_validator_credit(
				RuntimeOrigin::signed(account(1)),
				1,
				authority(Ed25519Keyring::Alice)
			),
			Error::<Test>::NoValidatorPayout
		);
		assert!(ValidatorCredits::<Test>::get(1, authority(Ed25519Keyring::Alice)).is_some());
		assert_ok!(register(Ed25519Keyring::Alice, 42, 0));
		assert_ok!(MfcRewards::claim_validator_credit(
			RuntimeOrigin::signed(account(1)),
			1,
			authority(Ed25519Keyring::Alice)
		));
		assert_eq!(ValidatorPaid::<Test>::get(), validator / 2);
		assert_eq!(ValidatorCreditTotal::<Test>::get(), validator - validator / 2);
		assert!(ValidatorCredits::<Test>::get(1, authority(Ed25519Keyring::Bob)).is_some());
		assert_noop!(
			MfcRewards::claim_validator_credit(
				RuntimeOrigin::signed(account(1)),
				1,
				authority(Ed25519Keyring::Alice)
			),
			Error::<Test>::NoValidatorCredit
		);
		assert_emission_invariant();
	});
}

#[test]
fn forged_signature_and_noncanonical_target_are_rejected() {
	new_test_ext(true).execute_with(|| {
		finalize(1, 42);
		finalize(2, 42);
		finalize(3, 42);
		let encoded = justification(1, 7, &[Ed25519Keyring::Alice, Ed25519Keyring::Bob]);
		assert_noop!(
			MfcRewards::release_validator_reward(
				RuntimeOrigin::signed(account(1)),
				1,
				encoded.clone(),
				1,
				0
			),
			Error::<Test>::EvidenceWeightMismatch
		);
		let mut proof = GrandpaJustification::<HeaderOf<Test>>::decode_all(&mut &encoded[..]).unwrap();
		proof.round = 8;
		assert_noop!(release(1, proof.encode()), Error::<Test>::InvalidValidatorSignature);
		frame_system::BlockHash::<Test>::insert(1, H256::repeat_byte(0xff));
		assert_noop!(release(1, encoded.clone()), Error::<Test>::NonCanonicalTarget);
		assert!(ValidatorReleases::<Test>::get(1).is_none());
		assert_eq!(ValidatorPaid::<Test>::get(), 0);
		frame_system::BlockHash::<Test>::insert(1, target_hash(1));
		assert_ok!(release(1, encoded.clone()));
		assert_noop!(release(1, encoded), Error::<Test>::AlreadyReleased);
		assert_emission_invariant();
	});
}

#[test]
fn insufficient_quorum_unknown_and_duplicate_validators_are_rejected() {
	new_test_ext(true).execute_with(|| {
		finalize(1, 42);
		finalize(2, 42);
		finalize(3, 42);
		assert_noop!(
			release(1, justification(1, 7, &[Ed25519Keyring::Alice])),
			Error::<Test>::InvalidJustification
		);
		assert_noop!(
			release(1, justification(1, 7, &[Ed25519Keyring::Charlie])),
			Error::<Test>::UnknownValidator
		);
		assert_noop!(
			release(1, justification(1, 7, &[Ed25519Keyring::Alice, Ed25519Keyring::Alice])),
			Error::<Test>::DuplicateValidator
		);
	});
}

#[test]
fn reward_requires_maturation_and_payout_registration_is_replay_protected() {
	new_test_ext(true).execute_with(|| {
		finalize(1, 42);
		assert_noop!(
			release(1, justification(1, 7, &[Ed25519Keyring::Alice, Ed25519Keyring::Bob])),
			Error::<Test>::RewardNotMature
		);
		assert_ok!(register(Ed25519Keyring::Alice, 42, 0));
		assert_noop!(register(Ed25519Keyring::Alice, 43, 0), Error::<Test>::StalePayoutRegistration);
		assert_eq!(ValidatorPayouts::<Test>::get(0, authority(Ed25519Keyring::Alice)), Some(account(42)));
	});
}

#[test]
fn pruning_preserves_claimable_credit_and_blocks_replay() {
	new_test_ext(true).execute_with(|| {
		finalize(1, 42);
		finalize(2, 42);
		finalize(3, 42);
		assert_ok!(release(1, justification(1, 7, &[Ed25519Keyring::Alice, Ed25519Keyring::Bob])));
		assert!(ValidatorReleases::<Test>::get(1).is_some());
		finalize(4, 42);
		finalize(5, 42);
		assert!(ValidatorObligations::<Test>::get(1).is_none());
		assert!(ValidatorReleases::<Test>::get(1).is_none());
		assert!(ValidatorCredits::<Test>::get(1, authority(Ed25519Keyring::Alice)).is_some());
		assert_noop!(
			release(1, justification(1, 7, &[Ed25519Keyring::Alice, Ed25519Keyring::Bob])),
			Error::<Test>::UnknownValidatorObligation
		);
		assert_ok!(register(Ed25519Keyring::Alice, 42, 0));
		let before = ValidatorCreditTotal::<Test>::get();
		assert_ok!(MfcRewards::claim_validator_credit(
			RuntimeOrigin::signed(account(1)),
			1,
			authority(Ed25519Keyring::Alice)
		));
		assert!(ValidatorCreditTotal::<Test>::get() < before);
		assert_emission_invariant();
	});
}

#[test]
fn authority_transition_pays_old_set_then_rotates_to_new_set() {
	new_test_ext(true).execute_with(|| {
		finalize(1, 42);
		assert_ok!(Grandpa::schedule_change(
			vec![(authority(Ed25519Keyring::Charlie), 1)],
			1,
			None
		));
		pallet_grandpa::CurrentSetId::<Test>::put(1);
		finalize(2, 42);
		finalize(3, 42);
		assert_eq!(ValidatorObligations::<Test>::get(2).unwrap().set_id, 0);
		assert_eq!(ValidatorObligations::<Test>::get(3).unwrap().set_id, 1);
		finalize(4, 42);
		assert_ok!(release(2, justification_for_set(
			2,
			7,
			0,
			&[Ed25519Keyring::Alice, Ed25519Keyring::Bob]
		)));
		finalize(5, 42);
		assert_noop!(
			release(3, justification_for_set(3, 8, 0, &[Ed25519Keyring::Alice, Ed25519Keyring::Bob])),
			Error::<Test>::UnknownValidator
		);
		assert_ok!(release(3, justification_for_set(3, 8, 1, &[Ed25519Keyring::Charlie])));
		assert_eq!(ValidatorSets::<Test>::get(1).unwrap().to_vec(), vec![(authority(Ed25519Keyring::Charlie), 1)]);
		assert_emission_invariant();
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

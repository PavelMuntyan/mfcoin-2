use super::*;

use alloc::vec;
use crate::Pallet as MfcRewards;
use finality_grandpa::{Commit, Message, Precommit, SignedPrecommit};
use frame_benchmarking::v2::*;
use frame_support::traits::Hooks;
use frame_system::RawOrigin;
use sp_consensus_grandpa::{localized_payload, AuthorityId, AuthoritySignature, GrandpaJustification};

fn authority(index: u8) -> (ed25519_zebra::SigningKey, AuthorityId) {
	let pair = ed25519_zebra::SigningKey::from([index; 32]);
	let public: [u8; 32] = ed25519_zebra::VerificationKey::from(&pair).into();
	let id: AuthorityId = sp_core::ed25519::Public::from_raw(public).into();
	(pair, id)
}

fn sign(pair: &ed25519_zebra::SigningKey, payload: &[u8]) -> AuthoritySignature {
	sp_core::ed25519::Signature::from_raw(pair.sign(payload).to_bytes()).into()
}

fn bounded_authorities<T: Config>(
	authorities: sp_consensus_grandpa::AuthorityList,
) -> pallet_grandpa::BoundedAuthorityList<T::MaxAuthorities> {
	authorities.try_into().expect("benchmark authority count respects configured bound")
}

fn setup_set<T: Config>(count: u8) -> Vec<(ed25519_zebra::SigningKey, AuthorityId, T::AccountId)> {
	let entries = (1..=count)
		.map(|index| {
			let (pair, id) = authority(index);
			let payout: T::AccountId = account("payout", index.into(), 0);
			(pair, id, payout)
		})
		.collect::<Vec<_>>();
	let authorities = entries
		.iter()
		.map(|(_, id, _)| (id.clone(), 1))
		.collect::<Vec<_>>();
	ValidatorSets::<T>::insert(0, bounded_authorities::<T>(authorities));
	LatestValidatorSetId::<T>::put(0);
	entries
}

fn setup_worst_case_evidence<T: Config>(
	authority_count: u32,
	requested_ancestry_count: u32,
) -> (BlockNumberFor<T>, Vec<u8>, u32, u32)
where
	BlockNumberFor<T>: finality_grandpa::BlockNumberOps,
{
	let entries = setup_set::<T>(authority_count as u8);
	let ancestry_count = if authority_count > 1 { requested_ancestry_count } else { 0 };
	let highest = 65 + ancestry_count;
	let target = 65u32.into();
	let mut parent_hash = T::Hash::default();
	let mut target_hash = T::Hash::default();
	let mut ancestors = Vec::new();
	for number in 1u32..=highest {
		let header = HeaderOf::<T> {
			parent_hash,
			number: number.into(),
			state_root: T::Hash::default(),
			extrinsics_root: T::Hash::default(),
			digest: Default::default(),
		};
		parent_hash = header.hash();
		if number == 65 {
			target_hash = parent_hash;
		} else if number > 65 {
			ancestors.push(header);
		}
	}
	let precommits = entries
		.iter()
		.enumerate()
		.map(|(index, (pair, id, _))| {
			let precommit = if ancestry_count > 0 && index + 1 == entries.len() {
				Precommit::new(parent_hash, highest.into())
			} else {
				Precommit::new(target_hash, target)
			};
			let message = Message::Precommit(precommit.clone());
			let payload = localized_payload(7, 0, &message);
			SignedPrecommit {
				precommit,
				signature: sign(pair, &payload),
				id: id.clone(),
			}
		})
		.collect();
	let proof = GrandpaJustification::<HeaderOf<T>> {
		round: 7,
		commit: Commit {
			target_hash,
			target_number: target,
			precommits,
		},
		votes_ancestries: ancestors,
	};
	Enabled::<T>::set(true);
	ActivationBlock::<T>::set(0u32.into());
	TreasuryAccount::<T>::set(Some(whitelisted_caller()));
	ValidatorObligations::<T>::insert(
		target,
		ValidatorObligation {
			amount: 1_000_000_000_000u128.saturated_into(),
			set_id: 0,
		},
	);
	for (_, id, payout) in entries {
		ValidatorPayouts::<T>::insert(0, id, payout);
	}
	frame_system::BlockHash::<T>::insert(target, target_hash);
	frame_system::Pallet::<T>::set_block_number(100u32.into());
	(target, proof.encode(), authority_count, ancestry_count)
}

#[benchmarks(where BlockNumberFor<T>: finality_grandpa::BlockNumberOps)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn release_validator_reward(a: Linear<1, 32>, h: Linear<0, 64>) {
		let (target, proof, signer_count, ancestry_count) = setup_worst_case_evidence::<T>(a, h);
		let relayer: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		release_validator_reward(RawOrigin::Signed(relayer), target, proof, signer_count, ancestry_count);

		assert!(ValidatorReleases::<T>::get(target).is_some());
	}

	#[benchmark]
	fn register_validator_payout() {
		let entries = setup_set::<T>(1);
		let (pair, id, payout) = entries[0].clone();
		let relayer: T::AccountId = whitelisted_caller();
		let payload = validator_payout_payload(
			frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::zero()),
			0,
			&id,
			&payout,
			0,
		);
		let signature = sign(&pair, &payload);
		#[extrinsic_call]
		register_validator_payout(RawOrigin::Signed(relayer), 0, id.clone(), payout, 0, signature);

		assert_eq!(ValidatorPayoutNonces::<T>::get(0, id), 1);
	}

	#[benchmark]
	fn claim_validator_credit() {
		let entries = setup_set::<T>(1);
		let (_, id, payout) = entries[0].clone();
		let relayer: T::AccountId = whitelisted_caller();
		let target = 1u32.into();
		let amount = 1_000_000_000u128.saturated_into();
		ValidatorPayouts::<T>::insert(0, id.clone(), payout);
		ValidatorCredits::<T>::insert(
			target,
			id.clone(),
			ValidatorCredit { amount, set_id: 0 },
		);
		ValidatorCreditTotal::<T>::set(amount);
		#[extrinsic_call]
		claim_validator_credit(RawOrigin::Signed(relayer), target, id.clone());

		assert!(ValidatorCredits::<T>::get(target, id).is_none());
	}

	#[benchmark]
	fn on_finalize() {
		let block: BlockNumberFor<T> = 1201u32.into();
		let mature = block.saturating_sub(T::RewardMaturation::get());
		let expired = block.saturating_sub(T::EvidenceRetention::get());
		let payout: T::AccountId = whitelisted_caller();
		Enabled::<T>::set(true);
		ActivationBlock::<T>::set(0u32.into());
		TreasuryAccount::<T>::set(Some(payout.clone()));
		frame_system::Pallet::<T>::set_block_number(block);
		let mut digest = vec![PAYOUT_VERSION];
		digest.extend_from_slice(payout.encode().as_slice());
		frame_system::Pallet::<T>::deposit_log(sp_runtime::generic::DigestItem::PreRuntime(
			POW_ENGINE_ID,
			digest,
		));
		Obligations::<T>::insert(
			mature,
			RewardObligation {
				pow_payout: payout,
				pow_amount: 1u32.into(),
				treasury_amount: 1u32.into(),
				validator_amount: 1u32.into(),
			},
		);
		ValidatorObligations::<T>::insert(
			expired,
			ValidatorObligation { amount: 1u32.into(), set_id: 0 },
		);
		ValidatorReleases::<T>::insert(
			expired,
			ValidatorRelease {
				round: 0,
				set_id: 0,
				signers: 1,
				paid: 0u32.into(),
				credited: 1u32.into(),
				treasury_remainder: 0u32.into(),
			},
		);
		#[block]
		{
			MfcRewards::<T>::on_finalize(block);
		}

		assert!(ValidatorObligations::<T>::get(expired).is_none());
	}

	impl_benchmark_test_suite!(MfcRewards, crate::mock::new_test_ext(true), crate::mock::Test);
}

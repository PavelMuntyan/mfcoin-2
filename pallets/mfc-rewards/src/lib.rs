#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeAll, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::{RuntimeDebug, TypeInfo},
	traits::{Currency, Get},
};
use sp_consensus_grandpa::{AuthorityId, AuthoritySignature, SetId};
use sp_core::ByteArray;
use sp_runtime::{
	generic::DigestItem,
	traits::{SaturatedConversion, Saturating, Zero},
};

pub use pallet::*;
pub mod economics;
pub mod evidence;
pub mod weights;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
use economics::{block_subsidy, split_subsidy, BLOCKS_PER_YEAR};
use frame_system::pallet_prelude::BlockNumberFor;

pub const POW_ENGINE_ID: [u8; 4] = *b"pow_";
pub const PAYOUT_VERSION: u8 = 0;
pub const VALIDATOR_PAYOUT_DOMAIN: &[u8] = b"mfc:validator-payout:v0";

pub type HeaderOf<T> = sp_runtime::generic::Header<
	BlockNumberFor<T>,
	<T as frame_system::Config>::Hashing,
>;

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct RewardObligation<AccountId, Balance> {
	pub pow_payout: AccountId,
	pub pow_amount: Balance,
	pub treasury_amount: Balance,
	pub validator_amount: Balance,
}

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct ValidatorObligation<Balance> {
	pub amount: Balance,
	pub set_id: SetId,
}

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct ValidatorCredit<Balance> {
	pub amount: Balance,
	pub set_id: SetId,
}

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct ValidatorRelease<Balance> {
	pub round: u64,
	pub set_id: SetId,
	pub signers: u32,
	pub paid: Balance,
	pub credited: Balance,
	pub treasury_remainder: Balance,
}

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct RewardTotals<Balance> {
	pub scheduled: Balance,
	pub minted: Balance,
	pub validator_accrued: Balance,
	pub validator_paid: Balance,
	pub validator_credit: Balance,
	pub pow_paid: Balance,
	pub treasury_paid: Balance,
}

pub fn validator_payout_payload<AccountId: Encode, Hash: Encode>(
	genesis_hash: Hash,
	set_id: SetId,
	authority: &AuthorityId,
	payout: &AccountId,
	nonce: u64,
) -> alloc::vec::Vec<u8> {
	(
		VALIDATOR_PAYOUT_DOMAIN,
		genesis_hash,
		set_id,
		authority,
		payout,
		nonce,
	)
		.encode()
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::weights::WeightInfo as RewardWeightInfo;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_grandpa::Config {
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type Currency: Currency<Self::AccountId>;
		type RewardMaturation: Get<BlockNumberFor<Self>>;
		type EvidenceRetention: Get<BlockNumberFor<Self>>;
		type MaxEncodedEvidence: Get<u32>;
		type WeightInfo: weights::WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type Enabled<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::storage]
	pub type ActivationBlock<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::storage]
	pub type TreasuryAccount<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	pub type Obligations<T: Config> = StorageMap<
		_,
		Twox64Concat,
		BlockNumberFor<T>,
		RewardObligation<T::AccountId, BalanceOf<T>>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type ValidatorObligations<T: Config> = StorageMap<
		_,
		Twox64Concat,
		BlockNumberFor<T>,
		ValidatorObligation<BalanceOf<T>>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type ValidatorSets<T: Config> = StorageMap<
		_,
		Twox64Concat,
		SetId,
		pallet_grandpa::BoundedAuthorityList<<T as pallet_grandpa::Config>::MaxAuthorities>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type ValidatorReleases<T: Config> = StorageMap<
		_,
		Twox64Concat,
		BlockNumberFor<T>,
		ValidatorRelease<BalanceOf<T>>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type ValidatorPayouts<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		SetId,
		Blake2_128Concat,
		AuthorityId,
		T::AccountId,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type ValidatorPayoutNonces<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		SetId,
		Blake2_128Concat,
		AuthorityId,
		u64,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type ValidatorCredits<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		BlockNumberFor<T>,
		Blake2_128Concat,
		AuthorityId,
		ValidatorCredit<BalanceOf<T>>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type LatestValidatorSetId<T: Config> = StorageValue<_, SetId, OptionQuery>;

	#[pallet::storage]
	pub type ScheduledTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::storage]
	pub type MintedTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::storage]
	pub type ValidatorAccrued<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::storage]
	pub type ValidatorPaid<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::storage]
	pub type ValidatorCreditTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::storage]
	pub type PowPaid<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::storage]
	pub type TreasuryPaid<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub enabled: bool,
		pub activation_block: BlockNumberFor<T>,
		pub treasury_account: Option<T::AccountId>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				enabled: false,
				activation_block: Default::default(),
				treasury_account: None,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			assert!(
				!self.enabled || self.treasury_account.is_some(),
				"treasury account required"
			);
			assert!(
				T::RewardMaturation::get() > Zero::zero(),
				"reward maturation must be positive"
			);
			assert!(
				T::EvidenceRetention::get() > T::RewardMaturation::get()
					&& T::EvidenceRetention::get() <= T::BlockHashCount::get(),
				"evidence retention must exceed maturation and fit canonical hashes"
			);
			Enabled::<T>::set(self.enabled);
			ActivationBlock::<T>::set(self.activation_block);
			TreasuryAccount::<T>::set(self.treasury_account.clone());
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		RewardScheduled {
			block: BlockNumberFor<T>,
			pow_payout: T::AccountId,
			subsidy: BalanceOf<T>,
		},
		RewardMatured {
			block: BlockNumberFor<T>,
			pow_payout: T::AccountId,
			pow_amount: BalanceOf<T>,
			treasury_amount: BalanceOf<T>,
		},
		ValidatorPayoutRegistered {
			set_id: SetId,
			authority: AuthorityId,
			payout: T::AccountId,
			nonce: u64,
		},
		ValidatorRewardReleased {
			block: BlockNumberFor<T>,
			round: u64,
			signers: u32,
			paid: BalanceOf<T>,
			credited: BalanceOf<T>,
			treasury_remainder: BalanceOf<T>,
		},
		ValidatorCreditClaimed {
			block: BlockNumberFor<T>,
			authority: AuthorityId,
			payout: T::AccountId,
			amount: BalanceOf<T>,
		},
		ValidatorRewardHistoryPruned {
			block: BlockNumberFor<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		RewardsDisabled,
		UnknownValidatorObligation,
		UnknownValidatorSet,
		UnknownValidator,
		DuplicateValidator,
		InvalidValidatorSet,
		InvalidValidatorSignature,
		InvalidEvidenceEncoding,
		EvidenceTooLarge,
		EvidenceWeightMismatch,
		InvalidJustification,
		InvalidVoteAncestry,
		UnusedAncestryHeaders,
		TargetMismatch,
		NonCanonicalTarget,
		RewardNotMature,
		AlreadyReleased,
		ArithmeticOverflow,
		InvalidPayoutSignature,
		StalePayoutRegistration,
		NoValidatorCredit,
		NoValidatorPayout,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		BlockNumberFor<T>: finality_grandpa::BlockNumberOps,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::release_validator_reward(*signer_count, *ancestry_count))]
		pub fn release_validator_reward(
			origin: OriginFor<T>,
			target: BlockNumberFor<T>,
			encoded_justification: Vec<u8>,
			signer_count: u32,
			ancestry_count: u32,
		) -> DispatchResult
		where
			BlockNumberFor<T>: finality_grandpa::BlockNumberOps,
		{
			let _relayer = ensure_signed(origin)?;
			ensure!(Enabled::<T>::get(), Error::<T>::RewardsDisabled);
			ensure!(
				!ValidatorReleases::<T>::contains_key(target),
				Error::<T>::AlreadyReleased
			);
			ensure!(
				encoded_justification.len() <= T::MaxEncodedEvidence::get() as usize,
				Error::<T>::EvidenceTooLarge
			);
			let obligation = ValidatorObligations::<T>::get(target)
				.ok_or(Error::<T>::UnknownValidatorObligation)?;
			let current = frame_system::Pallet::<T>::block_number();
			ensure!(
				current >= target.saturating_add(T::RewardMaturation::get()),
				Error::<T>::RewardNotMature
			);
			let justification = sp_consensus_grandpa::GrandpaJustification::<HeaderOf<T>>::decode_all(
				&mut &encoded_justification[..],
			)
			.map_err(|_| Error::<T>::InvalidEvidenceEncoding)?;
			ensure!(
				justification.commit.precommits.len() as u32 == signer_count
					&& justification.votes_ancestries.len() as u32 == ancestry_count,
				Error::<T>::EvidenceWeightMismatch
			);
			ensure!(
				justification.commit.target_number == target,
				Error::<T>::TargetMismatch
			);
			ensure!(
				justification.votes_ancestries.len() <= 64
					&& justification.commit.precommits.len()
						<= <T as pallet_grandpa::Config>::MaxAuthorities::get() as usize,
				Error::<T>::EvidenceTooLarge
			);
			ensure!(
				frame_system::Pallet::<T>::block_hash(target)
					== justification.commit.target_hash,
				Error::<T>::NonCanonicalTarget
			);
			let authorities = ValidatorSets::<T>::get(obligation.set_id)
				.ok_or(Error::<T>::UnknownValidatorSet)?;
			let signers = evidence::verify_justification::<T>(
				&justification,
				obligation.set_id,
				&authorities,
			)?;
			let total_weight = signers.iter().try_fold(0u128, |total, (_, weight)| {
				total.checked_add(u128::from(*weight))
			});
			let total_weight = total_weight.ok_or(Error::<T>::ArithmeticOverflow)?;
			ensure!(total_weight > 0, Error::<T>::InvalidValidatorSet);

			let amount = obligation.amount.saturated_into::<u128>();
			let mut distributed = 0u128;
			let mut paid = 0u128;
			let mut credited = 0u128;
			for (authority, weight) in signers {
				let share = amount
					.checked_mul(u128::from(weight))
					.ok_or(Error::<T>::ArithmeticOverflow)?
					/ total_weight;
				distributed = distributed
					.checked_add(share)
					.ok_or(Error::<T>::ArithmeticOverflow)?;
				let share_balance = share.saturated_into::<BalanceOf<T>>();
				if let Some(payout) = ValidatorPayouts::<T>::get(obligation.set_id, &authority) {
					drop(T::Currency::deposit_creating(&payout, share_balance));
					paid = paid.checked_add(share).ok_or(Error::<T>::ArithmeticOverflow)?;
				} else if !share_balance.is_zero() {
					ValidatorCredits::<T>::insert(
						target,
						&authority,
						ValidatorCredit { amount: share_balance, set_id: obligation.set_id },
					);
					credited = credited
						.checked_add(share)
						.ok_or(Error::<T>::ArithmeticOverflow)?;
				}
			}
			let treasury_remainder = amount - distributed;
			let treasury = TreasuryAccount::<T>::get().expect("checked at genesis");
			if treasury_remainder > 0 {
				drop(T::Currency::deposit_creating(
					&treasury,
					treasury_remainder.saturated_into::<BalanceOf<T>>(),
				));
			}
			ValidatorPaid::<T>::mutate(|total| {
				*total = total.saturating_add(paid.saturated_into())
			});
			ValidatorCreditTotal::<T>::mutate(|total| {
				*total = total.saturating_add(credited.saturated_into())
			});
			TreasuryPaid::<T>::mutate(|total| {
				*total = total.saturating_add(treasury_remainder.saturated_into())
			});
			MintedTotal::<T>::mutate(|total| {
				*total = total.saturating_add((paid + treasury_remainder).saturated_into())
			});
			let release = ValidatorRelease {
				round: justification.round,
				set_id: obligation.set_id,
				signers: justification.commit.precommits.len() as u32,
				paid: paid.saturated_into(),
				credited: credited.saturated_into(),
				treasury_remainder: treasury_remainder.saturated_into(),
			};
			ValidatorReleases::<T>::insert(target, release);
			Self::deposit_event(Event::ValidatorRewardReleased {
				block: target,
				round: justification.round,
				signers: justification.commit.precommits.len() as u32,
				paid: paid.saturated_into(),
				credited: credited.saturated_into(),
				treasury_remainder: treasury_remainder.saturated_into(),
			});
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::register_validator_payout())]
		pub fn register_validator_payout(
			origin: OriginFor<T>,
			set_id: SetId,
			authority: AuthorityId,
			payout: T::AccountId,
			nonce: u64,
			signature: AuthoritySignature,
		) -> DispatchResult {
			let _relayer = ensure_signed(origin)?;
			let authorities = ValidatorSets::<T>::get(set_id)
				.ok_or(Error::<T>::UnknownValidatorSet)?;
			ensure!(
				authorities.iter().any(|(id, _)| id == &authority),
				Error::<T>::UnknownValidator
			);
			let expected_nonce = ValidatorPayoutNonces::<T>::get(set_id, &authority);
			ensure!(expected_nonce == nonce, Error::<T>::StalePayoutRegistration);
			let payload = validator_payout_payload(
				frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::zero()),
				set_id,
				&authority,
				&payout,
				nonce,
			);
			let public = sp_core::ed25519::Public::from_slice(authority.as_ref())
				.map_err(|_| Error::<T>::InvalidPayoutSignature)?;
			let signature = sp_core::ed25519::Signature::from_slice(signature.as_ref())
				.map_err(|_| Error::<T>::InvalidPayoutSignature)?;
			ensure!(
				sp_io::crypto::ed25519_verify(&signature, &payload, &public),
				Error::<T>::InvalidPayoutSignature
			);
			let next_nonce = nonce.checked_add(1).ok_or(Error::<T>::ArithmeticOverflow)?;
			ValidatorPayoutNonces::<T>::insert(set_id, &authority, next_nonce);
			ValidatorPayouts::<T>::insert(set_id, &authority, payout.clone());
			Self::deposit_event(Event::ValidatorPayoutRegistered {
				set_id,
				authority,
				payout,
				nonce,
			});
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::claim_validator_credit())]
		pub fn claim_validator_credit(
			origin: OriginFor<T>,
			target: BlockNumberFor<T>,
			authority: AuthorityId,
		) -> DispatchResult {
			let _relayer = ensure_signed(origin)?;
			let credit = ValidatorCredits::<T>::get(target, &authority)
				.ok_or(Error::<T>::NoValidatorCredit)?;
			let amount = credit.amount;
			let payout = ValidatorPayouts::<T>::get(credit.set_id, &authority)
				.ok_or(Error::<T>::NoValidatorPayout)?;
			ValidatorCredits::<T>::remove(target, &authority);
			ValidatorCreditTotal::<T>::mutate(|total| {
				*total = total.saturating_sub(amount)
			});
			ValidatorPaid::<T>::mutate(|total| *total = total.saturating_add(amount));
			MintedTotal::<T>::mutate(|total| *total = total.saturating_add(amount));
			drop(T::Currency::deposit_creating(&payout, amount));
			Self::deposit_event(Event::ValidatorCreditClaimed {
				block: target,
				authority,
				payout,
				amount,
			});
			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_block_number: BlockNumberFor<T>) -> Weight {
			<T as Config>::WeightInfo::on_finalize()
		}

		fn on_finalize(block_number: BlockNumberFor<T>) {
			if !Enabled::<T>::get() || block_number < ActivationBlock::<T>::get() {
				return;
			}
			let year = (block_number - ActivationBlock::<T>::get())
				.saturated_into::<u32>()
				.checked_div(BLOCKS_PER_YEAR)
				.expect("nonzero blocks per year");
			let subsidy = block_subsidy(year);
			if subsidy == 0 {
				return;
			}
			let pow_payout = payout_from_digest::<T>();
			let (pow_amount, validator_amount, treasury_amount) = split_subsidy(subsidy);
			let obligation = RewardObligation {
				pow_payout: pow_payout.clone(),
				pow_amount: pow_amount.saturated_into(),
				treasury_amount: treasury_amount.saturated_into(),
				validator_amount: validator_amount.saturated_into(),
			};
			Obligations::<T>::insert(block_number, obligation);
			let set_id = record_validator_set::<T>();
			ValidatorObligations::<T>::insert(
				block_number,
				ValidatorObligation {
					amount: validator_amount.saturated_into(),
					set_id,
				},
			);
			ScheduledTotal::<T>::mutate(|total| {
				*total = total.saturating_add(subsidy.saturated_into())
			});
			ValidatorAccrued::<T>::mutate(|total| {
				*total = total.saturating_add(validator_amount.saturated_into())
			});
			Self::deposit_event(Event::RewardScheduled {
				block: block_number,
				pow_payout,
				subsidy: subsidy.saturated_into(),
			});

			let mature = block_number.saturating_sub(T::RewardMaturation::get());
			if mature >= ActivationBlock::<T>::get() {
				if let Some(reward) = Obligations::<T>::take(mature) {
					let treasury = TreasuryAccount::<T>::get().expect("checked at genesis");
					let pow_paid = reward.pow_amount + reward.treasury_amount;
					drop(T::Currency::deposit_creating(
						&reward.pow_payout,
						reward.pow_amount,
					));
					drop(T::Currency::deposit_creating(
						&treasury,
						reward.treasury_amount,
					));
					MintedTotal::<T>::mutate(|total| *total = total.saturating_add(pow_paid));
					PowPaid::<T>::mutate(|total| *total = total.saturating_add(reward.pow_amount));
					TreasuryPaid::<T>::mutate(|total| {
						*total = total.saturating_add(reward.treasury_amount)
					});
					Self::deposit_event(Event::RewardMatured {
						block: mature,
						pow_payout: reward.pow_payout,
						pow_amount: reward.pow_amount,
						treasury_amount: reward.treasury_amount,
					});
				}
			}

			let expired = block_number.saturating_sub(T::EvidenceRetention::get());
			if expired > Zero::zero() && expired >= ActivationBlock::<T>::get() {
				let had_obligation = ValidatorObligations::<T>::contains_key(expired);
				let had_release = ValidatorReleases::<T>::contains_key(expired);
				ValidatorObligations::<T>::remove(expired);
				ValidatorReleases::<T>::remove(expired);
				if had_obligation || had_release {
					Self::deposit_event(Event::ValidatorRewardHistoryPruned { block: expired });
				}
			}
		}
	}

	fn record_validator_set<T: Config>() -> SetId {
		let current_set_id = pallet_grandpa::Pallet::<T>::current_set_id();
		let current_authorities = pallet_grandpa::Authorities::<T>::get();
		let Some(previous_set_id) = LatestValidatorSetId::<T>::get() else {
			ValidatorSets::<T>::insert(current_set_id, current_authorities);
			LatestValidatorSetId::<T>::put(current_set_id);
			return current_set_id;
		};
		if current_set_id > previous_set_id {
			if !pallet_grandpa::PendingChange::<T>::exists() {
				ValidatorSets::<T>::insert(current_set_id, current_authorities);
				LatestValidatorSetId::<T>::put(current_set_id);
			}
			return previous_set_id;
		}
		if !ValidatorSets::<T>::contains_key(current_set_id) {
			ValidatorSets::<T>::insert(current_set_id, current_authorities);
		}
		current_set_id
	}

	fn payout_from_digest<T: Config>() -> T::AccountId {
		let digest = frame_system::Pallet::<T>::digest();
		let mut payouts = digest.logs.iter().filter_map(|log| {
			if let DigestItem::PreRuntime(engine, data) = log {
				if *engine == POW_ENGINE_ID {
					return Some(data);
				}
			}
			None
		});
		let data = payouts.next().expect("PoW payout digest is required");
		assert!(payouts.next().is_none(), "multiple PoW payout digests");
		assert!(
			data.len() == 33 && data[0] == PAYOUT_VERSION,
			"invalid PoW payout digest"
		);
		T::AccountId::decode(&mut &data[1..]).expect("invalid payout AccountId")
	}
}

sp_api::decl_runtime_apis! {
	pub trait MfcRewardsApi<AccountId, Balance>
	where
		AccountId: Encode + Decode,
		Balance: Encode + Decode,
	{
		fn reward_totals() -> RewardTotals<Balance>;
		fn reward_obligation(block: u32) -> Option<RewardObligation<AccountId, Balance>>;
		fn validator_obligation(block: u32) -> Option<ValidatorObligation<Balance>>;
		fn validator_release(block: u32) -> Option<ValidatorRelease<Balance>>;
		fn validator_payout_nonce(set_id: SetId, authority: AuthorityId) -> u64;
		fn validator_payout(set_id: SetId, authority: AuthorityId) -> Option<AccountId>;
		fn validator_credit(block: u32, authority: AuthorityId) -> Option<Balance>;
	}
}

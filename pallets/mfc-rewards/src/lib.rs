#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::{RuntimeDebug, TypeInfo},
	traits::{Currency, Get},
};
use sp_runtime::{
	generic::DigestItem,
	traits::{SaturatedConversion, Saturating, Zero},
};

pub use pallet::*;
pub mod economics;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
use economics::{block_subsidy, split_subsidy, BLOCKS_PER_YEAR};

pub const POW_ENGINE_ID: [u8; 4] = *b"pow_";
pub const PAYOUT_VERSION: u8 = 0;

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct RewardObligation<AccountId, Balance> {
	pub pow_payout: AccountId,
	pub pow_amount: Balance,
	pub treasury_amount: Balance,
	pub validator_amount: Balance,
}

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct RewardTotals<Balance> {
	pub scheduled: Balance,
	pub minted: Balance,
	pub validator_accrued: Balance,
	pub pow_paid: Balance,
	pub treasury_paid: Balance,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type Currency: Currency<Self::AccountId>;
		type RewardMaturation: Get<BlockNumberFor<Self>>;
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
	pub type ScheduledTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::storage]
	pub type MintedTotal<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::storage]
	pub type ValidatorAccrued<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

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
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
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
			if mature < ActivationBlock::<T>::get() {
				return;
			}
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
	}
}

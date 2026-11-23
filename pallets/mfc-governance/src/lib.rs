#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::pallet_prelude::{RuntimeDebug, TypeInfo};
use frame_support::traits::UnfilteredDispatchable;
use sp_runtime::Saturating;
use sp_core::H256;

pub use pallet::*;

#[derive(Clone, Decode, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
pub enum Proposal<AccountId, BlockNumber> {
    SetCode {
        code: Vec<u8>,
        execute_at: BlockNumber,
    },
    NoteStalled {
        delay: BlockNumber,
        best_finalized_block_number: BlockNumber,
        execute_at: BlockNumber,
    },
    RotateGrandpa {
        next_authorities: pallet_grandpa::AuthorityList,
        in_blocks: BlockNumber,
        forced: Option<BlockNumber>,
        execute_at: BlockNumber,
    },
    SetGovernanceAccount {
        new: AccountId,
        execute_at: BlockNumber,
    },
}

impl<AccountId: Encode, BlockNumber: Encode> Proposal<AccountId, BlockNumber> {
    pub fn id(&self) -> H256 {
        sp_io::hashing::blake2_256(&self.encode()).into()
    }

    pub fn execute_at(&self) -> &BlockNumber {
        match self {
            Self::SetCode { execute_at, .. } |
            Self::NoteStalled { execute_at, .. } |
            Self::RotateGrandpa { execute_at, .. } |
            Self::SetGovernanceAccount { execute_at, .. } => execute_at,
        }
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::BuildGenesisConfig};
    use frame_system::{pallet_prelude::*, RawOrigin};

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_grandpa::Config {
        type MinimumDelay: Get<BlockNumberFor<Self>>;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type GovernanceAccount<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

    #[pallet::storage]
    pub type Proposals<T: Config> = StorageMap<_, Blake2_128Concat, H256, Proposal<T::AccountId, BlockNumberFor<T>>, OptionQuery>;

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub governance_account: Option<T::AccountId>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self { governance_account: None }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            if let Some(account) = &self.governance_account {
                GovernanceAccount::<T>::put(account);
            } else {
                GovernanceAccount::<T>::kill();
            }
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ProposalScheduled { proposal_id: H256, execute_at: BlockNumberFor<T> },
        ProposalExecuted { proposal_id: H256 },
    }

    #[pallet::error]
    pub enum Error<T> {
        GovernanceDisabled,
        Unauthorized,
        DelayTooShort,
        ProposalAlreadyExists,
        ProposalNotFound,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(frame_support::weights::Weight::from_parts(50_000_000, 1024))]
        pub fn set_code(
            origin: OriginFor<T>,
            code: Vec<u8>,
            execute_at: BlockNumberFor<T>,
        ) -> DispatchResult {
            Self::ensure_governance(origin)?;
            Self::schedule(Proposal::SetCode { code, execute_at })
        }

        #[pallet::call_index(1)]
        #[pallet::weight(frame_support::weights::Weight::from_parts(50_000_000, 1024))]
        pub fn note_stalled(
            origin: OriginFor<T>,
            delay: BlockNumberFor<T>,
            best_finalized_block_number: BlockNumberFor<T>,
            execute_at: BlockNumberFor<T>,
        ) -> DispatchResult {
            Self::ensure_governance(origin)?;
            Self::schedule(Proposal::NoteStalled { delay, best_finalized_block_number, execute_at })
        }

        #[pallet::call_index(2)]
        #[pallet::weight(frame_support::weights::Weight::from_parts(50_000_000, 1024))]
        pub fn rotate_grandpa(
            origin: OriginFor<T>,
            next_authorities: pallet_grandpa::AuthorityList,
            in_blocks: BlockNumberFor<T>,
            forced: Option<BlockNumberFor<T>>,
            execute_at: BlockNumberFor<T>,
        ) -> DispatchResult {
            Self::ensure_governance(origin)?;
            Self::schedule(Proposal::RotateGrandpa { next_authorities, in_blocks, forced, execute_at })
        }

        #[pallet::call_index(3)]
        #[pallet::weight(frame_support::weights::Weight::from_parts(50_000_000, 1024))]
        pub fn set_governance_account(
            origin: OriginFor<T>,
            new: T::AccountId,
            execute_at: BlockNumberFor<T>,
        ) -> DispatchResult {
            Self::ensure_governance(origin)?;
            Self::schedule(Proposal::SetGovernanceAccount { new, execute_at })
        }

        #[pallet::call_index(4)]
        #[pallet::weight(frame_support::weights::Weight::from_parts(50_000_000, 1024))]
        pub fn execute(origin: OriginFor<T>, proposal_id: H256) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            let proposal = Proposals::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(now >= *proposal.execute_at(), Error::<T>::DelayTooShort);
            Proposals::<T>::remove(proposal_id);

            match proposal {
                Proposal::SetCode { code, .. } => {
                    frame_system::Call::<T>::set_code { code }.dispatch_bypass_filter(RawOrigin::Root.into()).map_err(|error| error.error)?;
                },
                Proposal::NoteStalled { delay, best_finalized_block_number, .. } => {
                    pallet_grandpa::Call::<T>::note_stalled { delay, best_finalized_block_number }.dispatch_bypass_filter(RawOrigin::Root.into()).map_err(|error| error.error)?;
                },
                Proposal::RotateGrandpa { next_authorities, in_blocks, forced, .. } => {
                    pallet_grandpa::Pallet::<T>::schedule_change(next_authorities, in_blocks, forced)?;
                },
                Proposal::SetGovernanceAccount { new, .. } => {
                    GovernanceAccount::<T>::put(new);
                },
            }

            Self::deposit_event(Event::ProposalExecuted { proposal_id });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn ensure_governance(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let governance = GovernanceAccount::<T>::get().ok_or(Error::<T>::GovernanceDisabled)?;
            ensure!(who == governance, Error::<T>::Unauthorized);
            Ok(())
        }

        fn schedule(proposal: Proposal<T::AccountId, BlockNumberFor<T>>) -> DispatchResult {
            let now = frame_system::Pallet::<T>::block_number();
            let execute_at = *proposal.execute_at();
            ensure!(execute_at >= now.saturating_add(T::MinimumDelay::get()), Error::<T>::DelayTooShort);

            let proposal_id = proposal.id();
            ensure!(!Proposals::<T>::contains_key(proposal_id), Error::<T>::ProposalAlreadyExists);
            Proposals::<T>::insert(proposal_id, proposal);
            Self::deposit_event(Event::ProposalScheduled { proposal_id, execute_at });
            Ok(())
        }
    }
}

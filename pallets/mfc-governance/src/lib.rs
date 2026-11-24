#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::{RuntimeDebug, TypeInfo}, traits::{ConstU32, UnfilteredDispatchable}};
use sp_runtime::Saturating;
use sp_core::H256;

pub use pallet::*;

pub type RuntimeCode = frame_support::BoundedVec<u8, ConstU32<3_000_000>>;
pub type GovernanceAuthorities = frame_support::BoundedVec<(pallet_grandpa::AuthorityId, u64), ConstU32<32>>;

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub enum Proposal<AccountId, BlockNumber> {
    SetCode {
        code: RuntimeCode,
        execute_at: BlockNumber,
    },
    NoteStalled {
        delay: BlockNumber,
        best_finalized_block_number: BlockNumber,
        execute_at: BlockNumber,
    },
    RotateGrandpa {
        next_authorities: GovernanceAuthorities,
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

pub struct RuntimeGovernanceAccount<T>(core::marker::PhantomData<T>);

impl<T: Config> frame_support::traits::Get<Option<T::AccountId>> for RuntimeGovernanceAccount<T> {
    fn get() -> Option<T::AccountId> {
        GovernanceAccount::<T>::get()
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::BuildGenesisConfig};
    use frame_system::{pallet_prelude::*, RawOrigin};

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_grandpa::Config {
        type SetCodeDelay: Get<BlockNumberFor<Self>>;
        type NoteStalledDelay: Get<BlockNumberFor<Self>>;
        type RotateGrandpaDelay: Get<BlockNumberFor<Self>>;
        type SetGovernanceAccountDelay: Get<BlockNumberFor<Self>>;
        type ExecutionWindow: Get<BlockNumberFor<Self>>;
        type MaxScheduleAhead: Get<BlockNumberFor<Self>>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type GovernanceAccount<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

    #[pallet::storage]
    pub type GovernanceEpoch<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type Proposals<T: Config> = StorageMap<_, Blake2_128Concat, H256, Proposal<T::AccountId, BlockNumberFor<T>>, OptionQuery>;

    #[pallet::storage]
    pub type ProposalEpochs<T: Config> = StorageMap<_, Blake2_128Concat, H256, u64, ValueQuery>;

    #[pallet::storage]
    pub type ProposalExpiries<T: Config> = StorageMap<_, Blake2_128Concat, H256, BlockNumberFor<T>, OptionQuery>;

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
        ProposalScheduled { proposal_id: H256, execute_at: BlockNumberFor<T>, expires_at: BlockNumberFor<T> },
        ProposalCancelled { proposal_id: H256 },
        ProposalExecuted { proposal_id: H256 },
    }

    #[pallet::error]
    pub enum Error<T> {
        GovernanceDisabled,
        Unauthorized,
        DelayTooShort,
        ScheduleTooFar,
        ProposalAlreadyExists,
        ProposalNotFound,
        ProposalExpired,
        StaleProposal,
        EmptyAuthorities,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(Pallet::<T>::schedule_weight(code.len()))]
        pub fn set_code(
            origin: OriginFor<T>,
            code: RuntimeCode,
            execute_at: BlockNumberFor<T>,
        ) -> DispatchResult {
            Self::ensure_governance(origin)?;
            Self::schedule(Proposal::SetCode { code, execute_at })
        }

        #[pallet::call_index(1)]
        #[pallet::weight(Pallet::<T>::schedule_weight(0))]
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
        #[pallet::weight(Pallet::<T>::schedule_weight(0))]
        pub fn rotate_grandpa(
            origin: OriginFor<T>,
            next_authorities: GovernanceAuthorities,
            in_blocks: BlockNumberFor<T>,
            forced: Option<BlockNumberFor<T>>,
            execute_at: BlockNumberFor<T>,
        ) -> DispatchResult {
            Self::ensure_governance(origin)?;
            ensure!(!next_authorities.is_empty(), Error::<T>::EmptyAuthorities);
            Self::schedule(Proposal::RotateGrandpa { next_authorities, in_blocks, forced, execute_at })
        }

        #[pallet::call_index(3)]
        #[pallet::weight(Pallet::<T>::schedule_weight(0))]
        pub fn set_governance_account(
            origin: OriginFor<T>,
            new: T::AccountId,
            execute_at: BlockNumberFor<T>,
        ) -> DispatchResult {
            Self::ensure_governance(origin)?;
            Self::schedule(Proposal::SetGovernanceAccount { new, execute_at })
        }

        #[pallet::call_index(4)]
        #[pallet::weight(Pallet::<T>::execute_weight(*proposal_id))]
        pub fn execute(origin: OriginFor<T>, proposal_id: H256) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            let proposal = Proposals::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;
            ensure!(ProposalEpochs::<T>::get(proposal_id) == GovernanceEpoch::<T>::get(), Error::<T>::StaleProposal);
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(now >= *proposal.execute_at(), Error::<T>::DelayTooShort);
            let expires_at = ProposalExpiries::<T>::get(proposal_id)
                .unwrap_or_else(|| proposal.execute_at().saturating_add(T::ExecutionWindow::get()));
            ensure!(now <= expires_at, Error::<T>::ProposalExpired);
            Self::remove_proposal(proposal_id);

            match proposal {
                Proposal::SetCode { code, .. } => {
                    frame_system::Call::<T>::set_code { code: code.into_inner() }.dispatch_bypass_filter(RawOrigin::Root.into()).map_err(|error| error.error)?;
                },
                Proposal::NoteStalled { delay, best_finalized_block_number, .. } => {
                    pallet_grandpa::Call::<T>::note_stalled { delay, best_finalized_block_number }.dispatch_bypass_filter(RawOrigin::Root.into()).map_err(|error| error.error)?;
                },
                Proposal::RotateGrandpa { next_authorities, in_blocks, forced, .. } => {
                    pallet_grandpa::Pallet::<T>::schedule_change(next_authorities.into_inner(), in_blocks, forced)?;
                },
                Proposal::SetGovernanceAccount { new, .. } => {
                    GovernanceAccount::<T>::put(new);
                    GovernanceEpoch::<T>::mutate(|epoch| *epoch = epoch.saturating_add(1));
                },
            }

            Self::deposit_event(Event::ProposalExecuted { proposal_id });
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(Pallet::<T>::schedule_weight(0))]
        pub fn cancel(origin: OriginFor<T>, proposal_id: H256) -> DispatchResult {
            Self::ensure_governance(origin)?;
            ensure!(Proposals::<T>::contains_key(proposal_id), Error::<T>::ProposalNotFound);
            Self::remove_proposal(proposal_id);
            Self::deposit_event(Event::ProposalCancelled { proposal_id });
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
            let delay = match &proposal {
                Proposal::SetCode { .. } => T::SetCodeDelay::get(),
                Proposal::NoteStalled { .. } => T::NoteStalledDelay::get(),
                Proposal::RotateGrandpa { .. } => T::RotateGrandpaDelay::get(),
                Proposal::SetGovernanceAccount { .. } => T::SetGovernanceAccountDelay::get(),
            };
            ensure!(execute_at >= now.saturating_add(delay), Error::<T>::DelayTooShort);
            ensure!(execute_at <= now.saturating_add(T::MaxScheduleAhead::get()), Error::<T>::ScheduleTooFar);

            let proposal_id = proposal.id();
            let expires_at = execute_at.saturating_add(T::ExecutionWindow::get());
            ensure!(!Proposals::<T>::contains_key(proposal_id), Error::<T>::ProposalAlreadyExists);
            Proposals::<T>::insert(proposal_id, proposal);
            ProposalEpochs::<T>::insert(proposal_id, GovernanceEpoch::<T>::get());
            ProposalExpiries::<T>::insert(proposal_id, expires_at);
            Self::deposit_event(Event::ProposalScheduled { proposal_id, execute_at, expires_at });
            Ok(())
        }

        fn remove_proposal(proposal_id: H256) {
            Proposals::<T>::remove(proposal_id);
            ProposalEpochs::<T>::remove(proposal_id);
            ProposalExpiries::<T>::remove(proposal_id);
        }

        fn schedule_weight(code_len: usize) -> Weight {
            Weight::from_parts(50_000_000, 1024)
                .saturating_add(Weight::from_parts(code_len as u64 * 1_000, code_len as u64))
        }

        pub(crate) fn execute_weight(proposal_id: H256) -> Weight {
            let code_len = Proposals::<T>::get(proposal_id)
                .map(|proposal| match proposal {
                    Proposal::SetCode { code, .. } => code.len(),
                    _ => 0,
                })
                .unwrap_or(0);
            Weight::from_parts(200_000_000, 1024)
                .saturating_add(Weight::from_parts(code_len as u64 * 1_000, code_len as u64))
        }
    }
}

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::{RuntimeDebug, TypeInfo};
use sp_core::H256;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub const RECORD_VERSION: u16 = 1;
pub const FREELAND_INFO_ENGINE_ID: [u8; 4] = *b"MFCF";

const RECORD_ID_DOMAIN: &[u8] = b"MFC/freeland-record/v1";
const RECORD_LEAF_DOMAIN: &[u8] = b"MFC/freeland-record-leaf/v1";
const RECORD_NODE_DOMAIN: &[u8] = b"MFC/freeland-record-node/v1";
const EMPTY_RECORDS_DOMAIN: &[u8] = b"MFC/freeland-empty-records/v1";
const REGISTRY_LEAF_DOMAIN: &[u8] = b"MFC/freeland-registry-leaf/v1";
const REGISTRY_NODE_DOMAIN: &[u8] = b"MFC/freeland-registry-node/v1";
const EMPTY_REGISTRY_DOMAIN: &[u8] = b"MFC/freeland-empty-registry/v1";
const BLOCK_INFO_DOMAIN: &[u8] = b"MFC/freeland-block-info/v1";

#[derive(Clone, Copy, Decode, codec::DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, Ord, PartialEq, PartialOrd, RuntimeDebug, TypeInfo)]
pub enum RecordKind {
	#[codec(index = 0)]
	Passport,
	#[codec(index = 1)]
	Citizen,
	#[codec(index = 2)]
	Election,
	#[codec(index = 3)]
	Document,
	#[codec(index = 4)]
	Revocation,
}

#[derive(Clone, Copy, Decode, codec::DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub enum RecordAction {
	#[codec(index = 0)]
	Issue,
	#[codec(index = 1)]
	Renew,
	#[codec(index = 2)]
	Revoke,
	#[codec(index = 3)]
	Update,
	#[codec(index = 4)]
	Open,
	#[codec(index = 5)]
	Commit,
	#[codec(index = 6)]
	Reveal,
	#[codec(index = 7)]
	Close,
}

impl RecordKind {
	fn supports(self, action: RecordAction) -> bool {
		match self {
			Self::Passport | Self::Citizen => matches!(
				action,
				RecordAction::Issue | RecordAction::Renew | RecordAction::Revoke | RecordAction::Update
			),
			Self::Election => matches!(
				action,
				RecordAction::Open | RecordAction::Commit | RecordAction::Reveal | RecordAction::Close
			),
			Self::Document => {
				matches!(action, RecordAction::Issue | RecordAction::Revoke | RecordAction::Update)
			},
			Self::Revocation => action == RecordAction::Revoke,
		}
	}

	fn is_registry(self) -> bool {
		matches!(self, Self::Passport | Self::Citizen)
	}
}

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct FreelandRecord<AccountId> {
	pub version: u16,
	pub kind: RecordKind,
	pub action: RecordAction,
	pub subject_commitment: H256,
	pub payload_hash: H256,
	pub issuer: AccountId,
	pub nonce: u64,
	pub evidence_hash: H256,
}

impl<AccountId: Encode> FreelandRecord<AccountId> {
	pub fn id(&self) -> H256 {
		hash_with_domain(RECORD_ID_DOMAIN, &self.encode())
	}
}

#[derive(Clone, Copy, Decode, Encode, Eq, MaxEncodedLen, Ord, PartialEq, PartialOrd, RuntimeDebug, TypeInfo)]
pub struct RegistryEntry {
	pub kind: RecordKind,
	pub subject_commitment: H256,
	pub record_id: H256,
}

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct FreelandBlockInfo {
	pub version: u16,
	pub previous_info_root: H256,
	pub records_root: H256,
	pub registry_root: H256,
	pub governance_root: H256,
	pub records_count: u32,
	pub epoch: u64,
	pub validator_set_hash: H256,
}

impl FreelandBlockInfo {
	pub fn root(&self) -> H256 {
		hash_with_domain(BLOCK_INFO_DOMAIN, &self.encode())
	}
}

fn hash_with_domain(domain: &[u8], data: &[u8]) -> H256 {
	let mut payload = Vec::with_capacity(domain.len() + data.len());
	payload.extend_from_slice(domain);
	payload.extend_from_slice(data);
	H256(sp_io::hashing::blake2_256(&payload))
}

fn merkle_root(mut level: Vec<H256>, empty_domain: &[u8], node_domain: &[u8]) -> H256 {
	if level.is_empty() {
		return hash_with_domain(empty_domain, &[]);
	}
	while level.len() > 1 {
		let mut next = Vec::with_capacity(level.len().div_ceil(2));
		for pair in level.chunks(2) {
			let right = pair.get(1).unwrap_or(&pair[0]);
			let mut payload = Vec::with_capacity(64);
			payload.extend_from_slice(pair[0].as_bytes());
			payload.extend_from_slice(right.as_bytes());
			next.push(hash_with_domain(node_domain, &payload));
		}
		level = next;
	}
	level[0]
}

pub fn records_root(record_ids: &[H256]) -> H256 {
	let leaves = record_ids
		.iter()
		.map(|record_id| hash_with_domain(RECORD_LEAF_DOMAIN, record_id.as_bytes()))
		.collect();
	merkle_root(leaves, EMPTY_RECORDS_DOMAIN, RECORD_NODE_DOMAIN)
}

pub fn registry_root(entries: &[RegistryEntry]) -> H256 {
	let leaves = entries
		.iter()
		.map(|entry| hash_with_domain(REGISTRY_LEAF_DOMAIN, &entry.encode()))
		.collect();
	merkle_root(leaves, EMPTY_REGISTRY_DOMAIN, REGISTRY_NODE_DOMAIN)
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::generic::DigestItem;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type MaxRecordsPerBlock: Get<u32>;
		type MaxRegistryEntries: Get<u32>;
	}

	#[pallet::storage]
	pub type IssuerPermissions<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		RecordKind,
		Blake2_128Concat,
		T::AccountId,
		bool,
		ValueQuery,
	>;

	#[pallet::storage]
	pub type NextNonce<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

	#[pallet::storage]
	pub type LatestRecords<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		RecordKind,
		Blake2_128Concat,
		H256,
		FreelandRecord<T::AccountId>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type RegistryEntries<T: Config> =
		StorageValue<_, BoundedVec<RegistryEntry, T::MaxRegistryEntries>, ValueQuery>;

	#[pallet::storage]
	pub type TotalRecords<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::storage]
	pub type BlockRecordIds<T: Config> =
		StorageValue<_, BoundedVec<H256, T::MaxRecordsPerBlock>, ValueQuery>;

	#[pallet::storage]
	pub type LatestBlockInfo<T: Config> = StorageValue<_, FreelandBlockInfo, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		IssuerUpdated { kind: RecordKind, issuer: T::AccountId, allowed: bool },
		RecordSubmitted {
			record_id: H256,
			kind: RecordKind,
			action: RecordAction,
			subject_commitment: H256,
			payload_hash: H256,
			evidence_hash: H256,
			issuer: T::AccountId,
			nonce: u64,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		UnauthorizedIssuer,
		InvalidAction,
		InvalidNonce,
		NonceOverflow,
		RecordCountOverflow,
		TooManyRecords,
		RegistryFull,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(_block_number: BlockNumberFor<T>) {
			let record_ids = BlockRecordIds::<T>::take();
			let previous_info_root =
				LatestBlockInfo::<T>::get().map(|info| info.root()).unwrap_or_default();
			let info = FreelandBlockInfo {
				version: RECORD_VERSION,
				previous_info_root,
				records_root: records_root(&record_ids),
				registry_root: registry_root(&RegistryEntries::<T>::get()),
				governance_root: H256::zero(),
				records_count: u32::try_from(record_ids.len()).unwrap_or(u32::MAX),
				epoch: 0,
				validator_set_hash: H256::zero(),
			};
			let root = info.root();
			LatestBlockInfo::<T>::put(info);
			frame_system::Pallet::<T>::deposit_log(DigestItem::Other(
				(FREELAND_INFO_ENGINE_ID, root).encode(),
			));
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn set_issuer(
			origin: OriginFor<T>,
			kind: RecordKind,
			issuer: T::AccountId,
			allowed: bool,
		) -> DispatchResult {
			ensure_root(origin)?;
			IssuerPermissions::<T>::set(kind, issuer.clone(), allowed);
			Self::deposit_event(Event::IssuerUpdated { kind, issuer, allowed });
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().reads_writes(5, 7) + Weight::from_parts(75_000_000, 0))]
		pub fn submit_record(
			origin: OriginFor<T>,
			kind: RecordKind,
			action: RecordAction,
			subject_commitment: H256,
			payload_hash: H256,
			nonce: u64,
			evidence_hash: H256,
		) -> DispatchResult {
			let issuer = ensure_signed(origin)?;
			ensure!(IssuerPermissions::<T>::get(kind, &issuer), Error::<T>::UnauthorizedIssuer);
			ensure!(kind.supports(action), Error::<T>::InvalidAction);
			ensure!(NextNonce::<T>::get(&issuer) == nonce, Error::<T>::InvalidNonce);
			let next_nonce = nonce.checked_add(1).ok_or(Error::<T>::NonceOverflow)?;
			let next_total = TotalRecords::<T>::get()
				.checked_add(1)
				.ok_or(Error::<T>::RecordCountOverflow)?;
			ensure!(
				BlockRecordIds::<T>::get().len() < T::MaxRecordsPerBlock::get() as usize,
				Error::<T>::TooManyRecords
			);

			let record = FreelandRecord {
				version: RECORD_VERSION,
				kind,
				action,
				subject_commitment,
				payload_hash,
				issuer: issuer.clone(),
				nonce,
				evidence_hash,
			};
			let record_id = record.id();
			let registry_update = if kind.is_registry() {
				let mut entries = RegistryEntries::<T>::get();
				let position = entries.binary_search_by(|entry| {
					(entry.kind, entry.subject_commitment).cmp(&(kind, subject_commitment))
				});
				let entry = RegistryEntry { kind, subject_commitment, record_id };
				match position {
					Ok(index) => entries[index] = entry,
					Err(index) => {
						ensure!(
							entries.len() < T::MaxRegistryEntries::get() as usize,
							Error::<T>::RegistryFull
						);
						entries.try_insert(index, entry).map_err(|_| Error::<T>::RegistryFull)?;
					},
				}
				Some(entries)
			} else {
				None
			};

			BlockRecordIds::<T>::try_mutate(|ids| ids.try_push(record_id))
				.map_err(|_| Error::<T>::TooManyRecords)?;
			if let Some(entries) = registry_update {
				RegistryEntries::<T>::put(entries);
			}
			LatestRecords::<T>::set(kind, subject_commitment, Some(record));
			NextNonce::<T>::set(&issuer, next_nonce);
			TotalRecords::<T>::set(next_total);
			Self::deposit_event(Event::RecordSubmitted {
				record_id,
				kind,
				action,
				subject_commitment,
				payload_hash,
				evidence_hash,
				issuer,
				nonce,
			});
			Ok(())
		}
	}
}


sp_api::decl_runtime_apis! {
	pub trait FreelandApi<AccountId>
	where
		AccountId: Encode + Decode,
	{
		fn latest_block_info() -> Option<FreelandBlockInfo>;
		fn latest_record(
			kind: RecordKind,
			subject_commitment: H256,
		) -> Option<FreelandRecord<AccountId>>;
		fn registry_entries() -> Vec<RegistryEntry>;
	}
}

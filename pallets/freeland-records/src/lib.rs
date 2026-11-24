#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::{RuntimeDebug, TypeInfo};
use sp_core::H256;

pub use pallet::*;
mod registry;
pub use registry::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::*;

pub const RECORD_VERSION: u16 = 1;
pub const FREELAND_INFO_ENGINE_ID: [u8; 4] = *b"MFCF";

const RECORD_ID_DOMAIN: &[u8] = b"MFC/freeland-record/v1";
const RECORD_LEAF_DOMAIN: &[u8] = b"MFC/freeland-record-leaf/v1";
const RECORD_NODE_DOMAIN: &[u8] = b"MFC/freeland-record-node/v1";
const EMPTY_RECORDS_DOMAIN: &[u8] = b"MFC/freeland-empty-records/v1";
const BLOCK_INFO_DOMAIN: &[u8] = b"MFC/freeland-block-info/v1";

#[derive(Clone, Copy, Decode, codec::DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, Ord, PartialEq, PartialOrd, RuntimeDebug, TypeInfo)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
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
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
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

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FreelandRecordSerde {
	version: u16,
	kind: RecordKind,
	action: RecordAction,
	subject_commitment: H256,
	payload_hash: H256,
	issuer: sp_core::crypto::AccountId32,
	nonce: u64,
	evidence_hash: H256,
}

impl From<&FreelandRecord<sp_core::crypto::AccountId32>> for FreelandRecordSerde {
	fn from(record: &FreelandRecord<sp_core::crypto::AccountId32>) -> Self {
		Self {
			version: record.version,
			kind: record.kind,
			action: record.action,
			subject_commitment: record.subject_commitment,
			payload_hash: record.payload_hash,
			issuer: record.issuer.clone(),
			nonce: record.nonce,
			evidence_hash: record.evidence_hash,
		}
	}
}

impl serde::Serialize for FreelandRecord<sp_core::crypto::AccountId32> {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		FreelandRecordSerde::from(self).serialize(serializer)
	}
}

impl<'de> serde::Deserialize<'de> for FreelandRecord<sp_core::crypto::AccountId32> {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let record = FreelandRecordSerde::deserialize(deserializer)?;
		Ok(Self {
			version: record.version,
			kind: record.kind,
			action: record.action,
			subject_commitment: record.subject_commitment,
			payload_hash: record.payload_hash,
			issuer: record.issuer,
			nonce: record.nonce,
			evidence_hash: record.evidence_hash,
		})
	}
}

#[derive(Clone, Decode, Encode, Eq, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct IssuerChange<AccountId> {
	pub kind: RecordKind,
	pub issuer: AccountId,
	pub allowed: bool,
	pub execute_at: u32,
}

pub fn issuer_change_id<AccountId: Encode>(
	kind: RecordKind,
	issuer: AccountId,
	allowed: bool,
	execute_at: u32,
) -> H256 {
	hash_with_domain(b"MFC/issuer-change/v1", &(kind, issuer, allowed, execute_at).encode())
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

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, DefaultNoBound};
	use frame_system::pallet_prelude::*;
	use sp_runtime::{generic::DigestItem, traits::{SaturatedConversion, Saturating}};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type MaxRecordsPerBlock: Get<u32>;
		type GovernanceDelay: Get<u32>;
		type GovernanceAccount: Get<Option<Self::AccountId>>;
		type WeightInfo: WeightInfo;
	}

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		#[serde(default)]
		pub records: Vec<FreelandRecord<sp_core::crypto::AccountId32>>,
		#[serde(default)]
		pub governance_account: Option<sp_core::crypto::AccountId32>,
		#[serde(skip)]
		pub _phantom: core::marker::PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for record in &self.records {
				assert_eq!(record.version, RECORD_VERSION, "unsupported genesis record version");
				let record = FreelandRecord::<T::AccountId> {
					version: record.version,
					kind: record.kind,
					action: record.action,
					subject_commitment: record.subject_commitment,
					payload_hash: record.payload_hash,
					issuer: T::AccountId::decode(&mut &record.issuer.encode()[..])
						.expect("genesis issuer uses the runtime account encoding"),
					nonce: record.nonce,
					evidence_hash: record.evidence_hash,
				};
				LatestRecords::<T>::set(record.kind, record.subject_commitment, Some(record.clone()));
				let next_nonce = record.nonce.saturating_add(1);
				if NextNonce::<T>::get(&record.issuer) < next_nonce {
					NextNonce::<T>::set(&record.issuer, next_nonce);
				}
				Pallet::<T>::update_registry(RegistryEntry {
					kind: record.kind,
					subject_commitment: record.subject_commitment,
					record_id: record.id(),
				})
				.expect("genesis registry key must be unique");
			}
			assert_eq!(RegistryLeaves::<T>::iter().count(), self.records.len(), "duplicate genesis registry key");
			TotalRecords::<T>::set(self.records.len() as u64);
		}
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
	pub type PendingIssuerChanges<T: Config> =
		StorageMap<_, Blake2_128Concat, H256, IssuerChange<T::AccountId>, OptionQuery>;

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
	pub type RegistryLeaves<T: Config> =
		StorageMap<_, Blake2_128Concat, H256, RegistryEntry, OptionQuery>;

	#[pallet::storage]
	pub type RegistryNodes<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		u16,
		Blake2_128Concat,
		H256,
		H256,
		OptionQuery,
	>;

	#[pallet::type_value]
	pub fn DefaultRegistryRoot() -> H256 {
		empty_registry_root()
	}

	#[pallet::storage]
	pub type RegistryRoot<T: Config> = StorageValue<_, H256, ValueQuery, DefaultRegistryRoot>;

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
		IssuerChangeScheduled {
			proposal_id: H256,
			kind: RecordKind,
			issuer: T::AccountId,
			allowed: bool,
			execute_at: u32,
		},
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
		RegistryIndexCollision,
		UnauthorizedGovernance,
		ProposalAlreadyExists,
		ProposalNotFound,
		ProposalNotReady,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_block_number: BlockNumberFor<T>) -> Weight {
			T::WeightInfo::on_finalize()
		}

		fn on_finalize(_block_number: BlockNumberFor<T>) {
			let record_ids = BlockRecordIds::<T>::take();
			let previous_info_root =
				LatestBlockInfo::<T>::get().map(|info| info.root()).unwrap_or_default();
			let info = FreelandBlockInfo {
				version: RECORD_VERSION,
				previous_info_root,
				records_root: records_root(&record_ids),
				registry_root: RegistryRoot::<T>::get(),
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

	impl<T: Config> Pallet<T> {
		pub fn update_registry(entry: RegistryEntry) -> Result<(), Error<T>> {
			let index = registry_index(entry.kind, entry.subject_commitment);
			if let Some(existing) = RegistryLeaves::<T>::get(index) {
				ensure!(
					existing.kind == entry.kind &&
						existing.subject_commitment == entry.subject_commitment,
					Error::<T>::RegistryIndexCollision
				);
			}
			let root = update_registry_path(
				&entry,
				|level, path| RegistryNodes::<T>::get(level, path),
				|level, path, hash| RegistryNodes::<T>::set(level, path, Some(hash)),
			);
			RegistryLeaves::<T>::set(index, Some(entry));
			RegistryRoot::<T>::set(root);
			Ok(())
		}

		pub fn registry_proof(kind: RecordKind, subject_commitment: H256) -> RegistryProof {
			let index = registry_index(kind, subject_commitment);
			let entry = RegistryLeaves::<T>::get(index);
			build_registry_proof(kind, subject_commitment, entry, RegistryRoot::<T>::get(), |level, path| {
				RegistryNodes::<T>::get(level, path)
			})
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_issuer())]
		pub fn set_issuer(
			origin: OriginFor<T>,
			kind: RecordKind,
			issuer: T::AccountId,
			allowed: bool,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(T::GovernanceAccount::get() == Some(who), Error::<T>::UnauthorizedGovernance);
			let execute_at = frame_system::Pallet::<T>::block_number()
				.saturating_add(T::GovernanceDelay::get().saturated_into())
				.saturated_into::<u32>();
			let proposal_id = issuer_change_id(kind, issuer.clone(), allowed, execute_at);
			ensure!(!PendingIssuerChanges::<T>::contains_key(proposal_id), Error::<T>::ProposalAlreadyExists);
			PendingIssuerChanges::<T>::set(
				proposal_id,
				Some(IssuerChange { kind, issuer: issuer.clone(), allowed, execute_at }),
			);
			Self::deposit_event(Event::IssuerChangeScheduled { proposal_id, kind, issuer, allowed, execute_at });
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_issuer())]
		pub fn execute_issuer_change(origin: OriginFor<T>, proposal_id: H256) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			let change = PendingIssuerChanges::<T>::get(proposal_id).ok_or(Error::<T>::ProposalNotFound)?;
			ensure!(
				frame_system::Pallet::<T>::block_number().saturated_into::<u32>() >= change.execute_at,
				Error::<T>::ProposalNotReady
			);
			PendingIssuerChanges::<T>::remove(proposal_id);
			IssuerPermissions::<T>::set(change.kind, change.issuer.clone(), change.allowed);
			Self::deposit_event(Event::IssuerUpdated {
				kind: change.kind,
				issuer: change.issuer,
				allowed: change.allowed,
			});
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::submit_record())]
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
			if kind.is_registry() {
				Self::update_registry(RegistryEntry { kind, subject_commitment, record_id })?;
			}
			BlockRecordIds::<T>::try_mutate(|ids| ids.try_push(record_id))
				.map_err(|_| Error::<T>::TooManyRecords)?;
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
		fn registry_proof(kind: RecordKind, subject_commitment: H256) -> RegistryProof;
		fn block_records() -> Vec<(u32, FreelandRecord<AccountId>)>;
		fn block_records_page(offset: u32, limit: u32) -> Vec<(u32, FreelandRecord<AccountId>)>;
		fn total_records() -> u64;
		fn pending_issuer_changes() -> Vec<(H256, IssuerChange<AccountId>)>;
	}
}

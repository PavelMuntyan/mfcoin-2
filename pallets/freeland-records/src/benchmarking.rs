use super::*;

use crate::Pallet as FreelandRecords;
use frame_benchmarking::v2::*;
use frame_support::{traits::Hooks, BoundedVec};
use frame_system::RawOrigin;

fn benchmark_hash(value: u64) -> H256 {
	let mut bytes = [0u8; 32];
	bytes[24..].copy_from_slice(&value.to_be_bytes());
	H256::from_slice(&bytes)
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_issuer() {
		let issuer: T::AccountId = whitelisted_caller();
		#[extrinsic_call]
		set_issuer(RawOrigin::Root, RecordKind::Passport, issuer.clone(), true);

		assert!(IssuerPermissions::<T>::get(RecordKind::Passport, issuer));
	}

	#[benchmark]
	fn submit_record() {
		let issuer: T::AccountId = whitelisted_caller();
		IssuerPermissions::<T>::set(RecordKind::Passport, issuer.clone(), true);
		FreelandRecords::<T>::update_registry(RegistryEntry {
			kind: RecordKind::Passport,
			subject_commitment: benchmark_hash(1),
			record_id: benchmark_hash(1_000),
		})
		.expect("initial registry entry is valid");
		#[extrinsic_call]
		submit_record(
			RawOrigin::Signed(issuer),
			RecordKind::Passport,
			RecordAction::Issue,
			benchmark_hash(2),
			benchmark_hash(1),
			0,
			benchmark_hash(2),
		);

		assert_eq!(TotalRecords::<T>::get(), 1);
		assert!(FreelandRecords::<T>::registry_proof(RecordKind::Passport, benchmark_hash(2)).entry.is_some());
	}

	#[benchmark]
	fn on_finalize() {
		let record_ids: BoundedVec<H256, T::MaxRecordsPerBlock> = (0u32..64)
			.map(|index| benchmark_hash(u64::from(index) + 1))
			.collect::<Vec<_>>()
			.try_into()
			.expect("benchmark record count respects the configured bound");
		BlockRecordIds::<T>::put(record_ids);
		RegistryRoot::<T>::put(benchmark_hash(999));
		#[block]
		{
			FreelandRecords::<T>::on_finalize(1u32.into());
		}

		let info = LatestBlockInfo::<T>::get().expect("block info is written");
		assert_eq!(info.records_count, 64);
		assert_eq!(info.registry_root, RegistryRoot::<T>::get());
	}

	impl_benchmark_test_suite!(FreelandRecords, crate::mock::new_test_ext(), crate::mock::Test);
}

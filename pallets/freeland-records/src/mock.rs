use crate as pallet_freeland_records;
use frame_support::{derive_impl, traits::{ConstU32, ConstU64}};
use sp_core::Void;
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type FreelandRecords = pallet_freeland_records::Pallet<Test>;

	#[runtime::pallet_index(2)]
	pub type Grandpa = pallet_grandpa::Pallet<Test>;

	#[runtime::pallet_index(3)]
	pub type MfcGovernance = pallet_mfc_governance::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl pallet_grandpa::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MaxAuthorities = ConstU32<32>;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = ConstU64<0>;
	type KeyOwnerProof = Void;
	type EquivocationReportSystem = ();
}

impl pallet_mfc_governance::Config for Test {
	type SetCodeDelay = ConstU64<10>;
	type NoteStalledDelay = ConstU64<2>;
	type RotateGrandpaDelay = ConstU64<10>;
	type SetGovernanceAccountDelay = ConstU64<10>;
	type ExecutionWindow = ConstU64<20>;
	type MaxScheduleAhead = ConstU64<40>;
}

impl pallet_freeland_records::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type MaxRecordsPerBlock = ConstU32<64>;
	type GovernanceDelay = ConstU32<2>;
	type GovernanceAccount = pallet_mfc_governance::RuntimeGovernanceAccount<Test>;
	type WeightInfo = pallet_freeland_records::weights::SubstrateWeight<Test>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_mfc_governance::GenesisConfig::<Test> { governance_account: Some(1) }
		.assimilate_storage(&mut storage)
		.unwrap();
	storage.into()
}

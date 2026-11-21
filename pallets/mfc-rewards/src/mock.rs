use crate as pallet_mfc_rewards;
use frame_support::{
	derive_impl,
	traits::{ConstU32, ConstU64, VariantCountOf},
};
use sp_core::crypto::AccountId32;
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
	pub type Balances = pallet_balances::Pallet<Test>;

	#[runtime::pallet_index(2)]
	pub type MfcRewards = pallet_mfc_rewards::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId32;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId32>;
	type AccountData = pallet_balances::AccountData<u64>;
}

impl pallet_balances::Config for Test {
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type Balance = u64;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU64<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
	type DoneSlashHandler = ();
}

impl pallet_mfc_rewards::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type RewardMaturation = ConstU64<2>;
}

pub fn account(id: u64) -> AccountId32 {
	let mut bytes = [0u8; 32];
	bytes[..8].copy_from_slice(&id.to_le_bytes());
	AccountId32::new(bytes)
}

pub fn new_test_ext(enabled: bool) -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();
	pallet_mfc_rewards::GenesisConfig::<Test> {
		enabled,
		activation_block: 0,
		treasury_account: Some(account(99)),
	}
	.assimilate_storage(&mut storage)
	.unwrap();
	storage.into()
}

// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

// Substrate and Polkadot dependencies
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU128, ConstU32, ConstU64, ConstU8, VariantCountOf},
	weights::{
		constants::{RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND},
		ConstantMultiplier, Weight, WeightToFeeCoefficient, WeightToFeeCoefficients,
		WeightToFeePolynomial,
	},
};
use frame_system::limits::{BlockLength, BlockWeights};
use pallet_transaction_payment::{FungibleAdapter, Multiplier, TargetedFeeAdjustment};
use sp_runtime::{traits::One, FixedPointNumber, Perbill, Perquintill};
use sp_version::RuntimeVersion;

// Local module imports
use super::{
	AccountId, Balance, Balances, Block, BlockNumber, Hash, Nonce, PalletInfo, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeFreezeReason, RuntimeHoldReason, RuntimeOrigin, RuntimeTask,
	System, DAYS, EXISTENTIAL_DEPOSIT, MICRO_UNIT, SLOT_DURATION, UNIT, VERSION,
};

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
	pub const BlockHashCount: BlockNumber = 2400;
	pub const Version: RuntimeVersion = VERSION;

	/// We allow for 2 seconds of compute with a 6 second average block time.
	pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
		Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
		NORMAL_DISPATCH_RATIO,
	);
	pub RuntimeBlockLength: BlockLength = BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub const SS58Prefix: u8 = 42;
}

/// All migrations of the runtime, aside from the ones declared in the pallets.
///
/// This can be a tuple of types, each implementing `OnRuntimeUpgrade`.
#[allow(unused_parens)]
type SingleBlockMigrations = (
	pallet_assets::migration::next_asset_id::SetNextAssetId<ConstU32<1>, Runtime>,
);

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`SoloChainDefaultConfig`](`struct@frame_system::config_preludes::SolochainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
	/// The block type for the runtime.
	type Block = Block;
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = RuntimeBlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = RuntimeBlockLength;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The type for storing how many extrinsics an account has signed.
	type Nonce = Nonce;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// Version of the runtime.
	type Version = Version;
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// This is used as an identifier of the chain. 42 is the generic substrate prefix.
	type SS58Prefix = SS58Prefix;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type SingleBlockMigrations = SingleBlockMigrations;
}

impl pallet_grandpa::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;

	type WeightInfo = ();
	type MaxAuthorities = ConstU32<32>;
	type MaxNominators = ConstU32<0>;
	type MaxSetIdSessionEntries = ConstU64<0>;

	type KeyOwnerProof = sp_core::Void;
	type EquivocationReportSystem = ();
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
	type WeightInfo = ();
}

impl pallet_balances::Config for Runtime {
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type DoneSlashHandler = ();
}

impl pallet_assets::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type RemoveItemsLimit = ConstU32<100>;
	type AssetId = u32;
	type AssetIdParameter = u32;
	type ReserveData = ();
	type Currency = Balances;
	type CreateOrigin = frame_system::EnsureSigned<AccountId>;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type AssetDeposit = ConstU128<{ 100 * UNIT }>;
	type AssetAccountDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
	type MetadataDepositBase = ConstU128<{ 10 * EXISTENTIAL_DEPOSIT }>;
	type MetadataDepositPerByte = ConstU128<{ 100 * MICRO_UNIT }>;
	type ApprovalDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
	type StringLimit = ConstU32<64>;
	type Freezer = ();
	type Holder = ();
	type Extra = ();
	type CallbackHandle = pallet_assets::AutoIncAssetId<Runtime>;
	type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
}

pub struct MfcWeightToFee;

impl WeightToFeePolynomial for MfcWeightToFee {
	type Balance = Balance;

	fn polynomial() -> WeightToFeeCoefficients<Self::Balance> {
		WeightToFeeCoefficients::from(&[WeightToFeeCoefficient {
			coeff_integer: 0,
			coeff_frac: Perbill::from_percent(1),
			negative: false,
			degree: 1,
		}][..])
	}
}

parameter_types! {
	pub const TargetBlockFullness: Perquintill = Perquintill::from_percent(25);
	pub AdjustmentVariable: Multiplier = Multiplier::saturating_from_rational(1, 10_000);
	pub MinimumMultiplier: Multiplier = Multiplier::one();
	pub MaximumMultiplier: Multiplier = Multiplier::saturating_from_integer(100);
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = FungibleAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<5>;
	type WeightToFee = MfcWeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, ConstU128<{ 10 * MICRO_UNIT }>>;
	type FeeMultiplierUpdate = TargetedFeeAdjustment<
		Self,
		TargetBlockFullness,
		AdjustmentVariable,
		MinimumMultiplier,
		MaximumMultiplier
	>;
	type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

impl pallet_freeland_records::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MaxRecordsPerBlock = ConstU32<64>;
	type GovernanceDelay = ConstU32<20>;
	type GovernanceAccount = pallet_mfc_governance::RuntimeGovernanceAccount<Runtime>;
	type WeightInfo = pallet_freeland_records::weights::SubstrateWeight<Runtime>;
}

impl pallet_mfc_rewards::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type RewardMaturation = ConstU32<10>;
	type EvidenceRetention = ConstU32<1200>;
	type MaxEncodedEvidence = ConstU32<{ 256 * 1024 }>;
	type WeightInfo = pallet_mfc_rewards::weights::SubstrateWeight<Runtime>;
}

impl pallet_multisig::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type BlockNumberProvider = System;
	type Currency = Balances;
	type DepositBase = ConstU128<{ 500 * MICRO_UNIT }>;
	type DepositFactor = ConstU128<{ 100 * MICRO_UNIT }>;
	type MaxSignatories = ConstU32<16>;
	type WeightInfo = ();
}

impl pallet_mfc_governance::Config for Runtime {
	type SetCodeDelay = ConstU32<{ 2 * DAYS }>;
	type NoteStalledDelay = ConstU32<20>;
	type RotateGrandpaDelay = ConstU32<{ 2 * DAYS }>;
	type SetGovernanceAccountDelay = ConstU32<{ 2 * DAYS }>;
	type ExecutionWindow = ConstU32<{ DAYS / 4 }>;
	type MaxScheduleAhead = ConstU32<{ 7 * DAYS }>;
}


#[cfg(test)]
mod tests {
	use super::*;
	use crate::UNIT;
	use sp_runtime::traits::{Convert, One};

	#[test]
	fn weight_fee_is_one_percent_of_ref_time() {
		assert_eq!(
			pallet_transaction_payment::Pallet::<Runtime>::weight_to_fee(Weight::from_parts(
				WEIGHT_REF_TIME_PER_SECOND,
				0,
			)),
			10_000 * MICRO_UNIT
		);
	}

	#[test]
	fn large_payload_has_meaningful_length_fee() {
		let spam_fee = pallet_transaction_payment::Pallet::<Runtime>::length_to_fee(
			3 * 1024 * 1024,
		);

		assert_eq!(spam_fee, 3 * 1024 * 1024 * 10 * MICRO_UNIT);
		assert!(spam_fee > 30 * UNIT);
		assert!(pallet_transaction_payment::Pallet::<Runtime>::length_to_fee(200) < UNIT);
	}

	#[test]
	fn congestion_multiplier_responds_to_full_blocks() {
		sp_io::TestExternalities::default().execute_with(|| {
			let initial = Multiplier::one();

			assert_eq!(<Runtime as pallet_transaction_payment::Config>::FeeMultiplierUpdate::convert(initial), initial);

			System::set_block_consumed_resources(
				Weight::from_parts(2 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
				5 * 1024 * 1024,
			);

			let adjusted = <Runtime as pallet_transaction_payment::Config>::FeeMultiplierUpdate::convert(initial);

			assert!(adjusted > initial);
			assert!(adjusted <= MaximumMultiplier::get());
		});
	}
}

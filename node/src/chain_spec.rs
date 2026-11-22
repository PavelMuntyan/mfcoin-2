use pallet_freeland_records::{FreelandRecord, RecordAction, RecordKind};
use sc_service::{ChainType, Properties};
use serde::{Deserialize, Serialize};
use serde_json::json;
use solochain_template_runtime::WASM_BINARY;
use sp_core::{crypto::AccountId32, ByteArray, H256};
use sp_keyring::Sr25519Keyring;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

fn synthetic_hash(domain: &[u8], value: u64) -> H256 {
	let mut payload = domain.to_vec();
	payload.extend_from_slice(&value.to_le_bytes());
	H256(sp_io::hashing::blake2_256(&payload))
}

fn synthetic_records(
) -> Result<Option<Vec<FreelandRecord<solochain_template_runtime::AccountId>>>, String> {
	let Some(value) = std::env::var_os("MFCOIN_SYNTHETIC_REGISTRY") else {
		return Ok(None);
	};
	let count = value
		.to_str()
		.and_then(|value| value.parse::<u64>().ok())
		.ok_or_else(|| "MFCOIN_SYNTHETIC_REGISTRY must be an integer".to_string())?;
	if count > 33_000 {
		return Err(
			"MFCOIN_SYNTHETIC_REGISTRY is limited to 33,000 development records".to_string(),
		);
	}
	let issuer = Sr25519Keyring::Alice.to_account_id();
	let records = (1..=count)
		.map(|value| FreelandRecord {
			version: pallet_freeland_records::RECORD_VERSION,
			kind: if value % 2 == 0 {
				RecordKind::Citizen
			} else {
				RecordKind::Passport
			},
			action: RecordAction::Issue,
			subject_commitment: synthetic_hash(b"MFC/synthetic-subject/v1", value),
			payload_hash: synthetic_hash(b"MFC/synthetic-payload/v1", value),
			issuer: issuer.clone(),
			nonce: value - 1,
			evidence_hash: synthetic_hash(b"MFC/synthetic-evidence/v1", value),
		})
		.collect();
	Ok(Some(records))
}

pub fn development_chain_spec() -> Result<ChainSpec, String> {
	let builder = ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Development")
	.with_id("dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET);
	let builder = match synthetic_records()? {
		Some(records) => builder.with_genesis_config_patch(json!({
			"freelandRecords": { "records": records }
		})),
		None => builder,
	};
	Ok(builder.build())
}

pub fn local_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Local Testnet")
	.with_id("local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
	.build())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductionNetwork {
	name: String,
	id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductionRewards {
	activation_block: u32,
	treasury_account: AccountId32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductionConfig {
	validators: Vec<AccountId32>,
	governance_signatories: Vec<AccountId32>,
	endowed_accounts: Vec<(AccountId32, u128)>,
	#[serde(default)]
	network: Option<ProductionNetwork>,
	#[serde(default)]
	rewards: Option<ProductionRewards>,
}

fn validate_production_config(config: &ProductionConfig) -> Result<(), String> {
	if config.validators.is_empty() {
		return Err("Production requires at least one GRANDPA validator".to_string());
	}
	if config
		.validators
		.iter()
		.collect::<std::collections::BTreeSet<_>>()
		.len() != config.validators.len()
	{
		return Err("Production GRANDPA validators must be unique".to_string());
	}
	if config.governance_signatories.len() < 2 {
		return Err("Production governance requires at least two signatories".to_string());
	}
	if config.governance_signatories.len() > 16 {
		return Err("Production governance supports at most 16 signatories".to_string());
	}
	if !config
		.governance_signatories
		.windows(2)
		.all(|pair| pair[0] < pair[1])
	{
		return Err("Governance signatories must be unique and sorted by AccountId".to_string());
	}
	if config.endowed_accounts.is_empty() {
		return Err("Production requires endowed accounts for fees".to_string());
	}
	if config
		.endowed_accounts
		.iter()
		.any(|(_, balance)| *balance == 0)
	{
		return Err("Endowed account balances must be greater than zero".to_string());
	}
	if config
		.endowed_accounts
		.iter()
		.map(|(account, _)| account)
		.collect::<std::collections::BTreeSet<_>>()
		.len() != config.endowed_accounts.len()
	{
		return Err("Endowed accounts must be unique".to_string());
	}
	if let Some(network) = &config.network {
		if network.name.trim().is_empty()
			|| network.id.is_empty()
			|| !network
				.id
				.bytes()
				.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
		{
			return Err("Production network name and id must be explicit and safe".to_string());
		}
	}
	if let Some(rewards) = &config.rewards {
		if rewards.activation_block == 0 {
			return Err("Reward activation must follow initial finality".to_string());
		}
		if !config
			.endowed_accounts
			.iter()
			.any(|(account, _)| account == &rewards.treasury_account)
		{
			return Err("Reward treasury must be explicitly endowed".to_string());
		}
	}
	Ok(())
}

fn production_authorities(
	accounts: &[AccountId32],
) -> Result<Vec<(sp_consensus_grandpa::AuthorityId, u64)>, String> {
	accounts
		.iter()
		.map(|account| {
			let authority = sp_consensus_grandpa::AuthorityId::from_slice(account.as_ref())
				.map_err(|_| "Invalid GRANDPA validator public key".to_string())?;
			Ok((authority, 1u64))
		})
		.collect()
}

fn production_chain_spec_from_config(config: ProductionConfig) -> Result<ChainSpec, String> {
	validate_production_config(&config)?;
	let governance_account =
		pallet_multisig::Pallet::<solochain_template_runtime::Runtime>::multi_account_id(
			&config.governance_signatories,
			2,
		);
	let authorities = production_authorities(&config.validators)?;
	let network = config.network.unwrap_or(ProductionNetwork {
		name: "MFCoin 2.0".to_string(),
		id: "mfcoin2".to_string(),
	});
	let mut genesis = json!({
		"balances": { "balances": config.endowed_accounts },
		"grandpa": { "authorities": authorities },
		"freelandRecords": { "governanceAccount": governance_account },
	});
	if let Some(rewards) = config.rewards {
		genesis["mfcRewards"] = json!({
			"enabled": true,
			"activationBlock": rewards.activation_block,
			"treasuryAccount": rewards.treasury_account,
		});
	}
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Production wasm not available".to_string())?,
		None,
	)
	.with_properties(Properties::from_iter([
		("ss58Format".to_string(), json!(42)),
		("tokenDecimals".to_string(), json!(12)),
		("tokenSymbol".to_string(), json!("MFC"))
	]))
	.with_name(&network.name)
	.with_id(&network.id)
	.with_chain_type(ChainType::Live)
	.with_genesis_config_patch(genesis)
	.build())
}

pub fn production_chain_spec() -> Result<ChainSpec, String> {
	let path = std::env::var_os("MFCOIN_PRODUCTION_CONFIG").ok_or_else(|| {
		"MFCOIN_PRODUCTION_CONFIG is required for the production chain".to_string()
	})?;
	let data = std::fs::read_to_string(&path)
		.map_err(|error| format!("Could not read production config {path:?}: {error}"))?;
	let config: ProductionConfig = serde_json::from_str(&data)
		.map_err(|error| format!("Invalid production config {path:?}: {error}"))?;
	production_chain_spec_from_config(config)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn valid_config() -> ProductionConfig {
		ProductionConfig {
			validators: vec![
				AccountId32::new(sp_keyring::Ed25519Keyring::Alice.public().0),
				AccountId32::new(sp_keyring::Ed25519Keyring::Bob.public().0),
			],
			governance_signatories: vec![
				Sr25519Keyring::Bob.to_account_id(),
				Sr25519Keyring::Charlie.to_account_id(),
				Sr25519Keyring::Alice.to_account_id(),
			],
			endowed_accounts: vec![
				(Sr25519Keyring::Alice.to_account_id(), 1_000_000_000_000),
				(Sr25519Keyring::Bob.to_account_id(), 1_000_000_000_000),
			],
			network: None,
			rewards: None,
		}
	}

	#[test]
	fn production_config_is_fail_closed() {
		assert_eq!(validate_production_config(&valid_config()), Ok(()));
		let account = Sr25519Keyring::Alice.to_account_id();
		let cases = vec![
			(
				ProductionConfig {
					validators: vec![],
					..valid_config()
				},
				"Production requires at least one GRANDPA validator",
			),
			(
				ProductionConfig {
					validators: vec![account.clone(), account.clone()],
					..valid_config()
				},
				"Production GRANDPA validators must be unique",
			),
			(
				ProductionConfig {
					governance_signatories: vec![account.clone()],
					..valid_config()
				},
				"Production governance requires at least two signatories",
			),
			(
				ProductionConfig {
					governance_signatories: (1..=17)
						.map(|value| AccountId32::new([value; 32]))
						.collect(),
					..valid_config()
				},
				"Production governance supports at most 16 signatories",
			),
			(
				ProductionConfig {
					governance_signatories: vec![account.clone(), account.clone()],
					..valid_config()
				},
				"Governance signatories must be unique and sorted by AccountId",
			),
			(
				ProductionConfig {
					endowed_accounts: vec![],
					..valid_config()
				},
				"Production requires endowed accounts for fees",
			),
			(
				ProductionConfig {
					endowed_accounts: vec![(account.clone(), 0)],
					..valid_config()
				},
				"Endowed account balances must be greater than zero",
			),
			(
				ProductionConfig {
					endowed_accounts: vec![(account.clone(), 1), (account, 1)],
					..valid_config()
				},
				"Endowed accounts must be unique",
			),
		];
		for (config, expected) in cases {
			assert_eq!(
				validate_production_config(&config),
				Err(expected.to_string())
			);
		}

		let mut json = serde_json::to_value(valid_config()).expect("valid config serializes");
		json.as_object_mut()
			.expect("config is an object")
			.insert("unexpected".into(), json!(true));
		assert!(serde_json::from_value::<ProductionConfig>(json).is_err());

		let validators = valid_config().validators;
		let authorities = production_authorities(&validators).expect("valid authority keys");
		assert_eq!(
			serde_json::to_value(authorities).expect("authorities serialize"),
			json!([
				[validators[0].to_string(), 1],
				[validators[1].to_string(), 1]
			])
		);
	}

	#[test]
	fn reward_testnet_requires_explicit_activation_and_treasury() {
		let mut config = valid_config();
		config.network = Some(ProductionNetwork {
			name: "MFCoin Testnet 112".to_string(),
			id: "mfcoin-testnet-112".to_string(),
		});
		config.rewards = Some(ProductionRewards {
			activation_block: 12,
			treasury_account: Sr25519Keyring::Alice.to_account_id(),
		});
		assert_eq!(validate_production_config(&config), Ok(()));
		let spec = production_chain_spec_from_config(config.clone()).expect("production spec builds");
		let properties = spec.properties();
		assert_eq!(properties["ss58Format"], json!(42));
		assert_eq!(properties["tokenDecimals"], json!(12));
		assert_eq!(properties["tokenSymbol"], json!("MFC"));
		let mut inactive = config;
		inactive.rewards.as_mut().expect("rewards are configured").activation_block = 0;
		assert_eq!(
			validate_production_config(&inactive),
			Err("Reward activation must follow initial finality".to_string())
		);
		let mut external_treasury = inactive;
		external_treasury.rewards.as_mut().expect("rewards are configured").activation_block = 12;
		external_treasury.rewards.as_mut().expect("rewards are configured").treasury_account =
			Sr25519Keyring::Charlie.to_account_id();
		assert_eq!(
			validate_production_config(&external_treasury),
			Err("Reward treasury must be explicitly endowed".to_string())
		);
	}

	#[test]
	fn production_genesis_builds_storage() {
		let spec =
			production_chain_spec_from_config(valid_config()).expect("production spec builds");
		let storage = <dyn sc_service::ChainSpec>::build_storage(&spec)
			.expect("production genesis storage builds");
		assert!(!storage.top.is_empty());
	}
}

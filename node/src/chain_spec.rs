use pallet_freeland_records::{FreelandRecord, RecordAction, RecordKind};
use sc_service::ChainType;
use serde_json::json;
use solochain_template_runtime::WASM_BINARY;
use sp_core::H256;
use sp_keyring::Sr25519Keyring;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

fn synthetic_hash(domain: &[u8], value: u64) -> H256 {
	let mut payload = domain.to_vec();
	payload.extend_from_slice(&value.to_le_bytes());
	H256(sp_io::hashing::blake2_256(&payload))
}

fn synthetic_records() -> Result<Option<Vec<FreelandRecord<solochain_template_runtime::AccountId>>>, String> {
	let Some(value) = std::env::var_os("MFCOIN_SYNTHETIC_REGISTRY") else {
		return Ok(None);
	};
	let count = value
		.to_str()
		.and_then(|value| value.parse::<u64>().ok())
		.ok_or_else(|| "MFCOIN_SYNTHETIC_REGISTRY must be an integer".to_string())?;
	if count > 33_000 {
		return Err("MFCOIN_SYNTHETIC_REGISTRY is limited to 33,000 development records".to_string());
	}
	let issuer = Sr25519Keyring::Alice.to_account_id();
	let records = (1..=count)
		.map(|value| FreelandRecord {
			version: pallet_freeland_records::RECORD_VERSION,
			kind: if value % 2 == 0 { RecordKind::Citizen } else { RecordKind::Passport },
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

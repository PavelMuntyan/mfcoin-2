use codec::DecodeAll;
use jsonrpsee::{core::client::ClientT, http_client::HttpClientBuilder, rpc_params};
use pallet_mfc_rewards::validator_payout_payload;
use sp_consensus_grandpa::AuthorityId;
use sp_core::{crypto::AccountId32, Pair, H256};
use sp_keyring::Ed25519Keyring;
use subxt::{dynamic::Value, OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;

fn hex_hash(bytes: &[u8]) -> H256 {
	H256::from_slice(bytes)
}

fn justification_bytes(block: &serde_json::Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
	let entries = block["justifications"].as_array().ok_or("Missing justifications")?;
	for entry in entries {
		let is_grandpa = entry[0]
			.as_array()
			.map(|id| id.iter().map(|byte| byte.as_u64().unwrap_or_default()).collect::<Vec<_>>() == vec![70, 82, 78, 75])
			.unwrap_or(false);
		if is_grandpa {
			if let Some(bytes) = entry[1].as_array() {
				return bytes
					.iter()
					.map(|byte| {
						byte
							.as_u64()
							.and_then(|value| u8::try_from(value).ok())
							.ok_or_else(|| "Invalid FRNK byte".into())
					})
					.collect();
			}
			return Ok(sp_core::bytes::from_hex(entry[1].as_str().ok_or("Missing FRNK proof")?)?);
		}
	}
	Err("No FRNK justification in target block".into())
}

fn proof_counts(proof: &[u8]) -> Result<(u32, u32), Box<dyn std::error::Error>> {
	let justification = sc_consensus_grandpa::GrandpaJustification::<
		solochain_template_runtime::opaque::Block,
	>::decode_all(&mut &proof[..])?;
	Ok((
		justification.justification.commit.precommits.len() as u32,
		justification.justification.votes_ancestries.len() as u32,
	))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let url = std::env::var("DEVNET_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:9944".to_string());
	if !url.starts_with("ws://127.0.0.1:") && !url.starts_with("ws://localhost:") {
		return Err("This development example only connects to a local devnet".into());
	}
	let api = OnlineClient::<PolkadotConfig>::from_insecure_url(&url).await?;
	let http = HttpClientBuilder::default().build(url.replace("ws://", "http://"))?;
	let finalized_hash = http.request::<H256, _>("chain_getFinalizedHead", rpc_params![]).await?;
	let finalized_header = http
		.request::<serde_json::Value, _>("chain_getHeader", rpc_params![finalized_hash])
		.await?;
	let finalized = u32::from_str_radix(
		finalized_header["number"].as_str().ok_or("Missing finalized number")?.trim_start_matches("0x"),
		16,
	)?;
	if finalized < 21 {
		return Err("Wait for at least 21 finalized blocks".into());
	}
	let target = finalized - 10;
	let target_hash = http.request::<H256, _>("chain_getBlockHash", rpc_params![target]).await?;
	let signed_block = http
		.request::<serde_json::Value, _>("chain_getBlock", rpc_params![target_hash])
		.await?;
	let proof = justification_bytes(&signed_block)?;
	let obligation = http
		.request::<serde_json::Value, _>("mfc_validatorReward", rpc_params![target, Option::<H256>::None])
		.await?;
	let amount = obligation["amount"].as_str().ok_or("Missing validator obligation")?.to_string();
	let set_id = obligation["setId"].as_u64().ok_or("Missing validator set id")?;
	let already_released = !obligation["released"].is_null();
	let (signer_count, ancestry_count) = proof_counts(&proof)?;
	let release = subxt::dynamic::tx(
		"MfcRewards",
		"release_validator_reward",
		vec![
			Value::u128(target.into()),
			Value::from_bytes(proof),
			Value::u128(signer_count.into()),
			Value::u128(ancestry_count.into()),
		],
	);
	let alice = dev::alice();
	if !already_released {
		api.tx()
			.sign_and_submit_then_watch_default(&release, &alice)
			.await?
			.wait_for_finalized_success()
			.await?;
	}
	let credit = http
		.request::<Option<String>, _>(
			"mfc_validatorCredit",
			rpc_params![target, hex_hash(Ed25519Keyring::Alice.public().as_ref()), Option::<H256>::None],
		)
		.await?
		.ok_or("Missing released credit")?;
	assert_eq!(credit, amount);
	let totals = http.request::<serde_json::Value, _>("mfc_rewardTotals", rpc_params![Option::<H256>::None]).await?;
	assert_eq!(totals["validatorPaid"].as_str(), Some("0"));
	let initial_credit = totals["validatorCredit"]
		.as_str()
		.ok_or("Missing validator credit total")?
		.parse::<u128>()?;
	assert!(initial_credit >= amount.parse::<u128>()?);
	println!("validator_release_finalized=true target={target} credit={credit}");

	let authority: AuthorityId = Ed25519Keyring::Alice.public().into();
	let payout = AccountId32::from(alice.public_key().0);
	let nonce = http
		.request::<String, _>(
			"mfc_validatorPayoutNonce",
			rpc_params![set_id, hex_hash(authority.as_ref()), Option::<H256>::None],
		)
		.await?
		.parse::<u64>()?;
	let genesis = http.request::<H256, _>("chain_getBlockHash", rpc_params![0]).await?;
	let payload = validator_payout_payload(genesis, set_id, &authority, &payout, nonce);
	let signature = Ed25519Keyring::Alice.pair().sign(&payload);
	let register = subxt::dynamic::tx(
		"MfcRewards",
		"register_validator_payout",
		vec![
			Value::u128(set_id.into()),
			Value::from_bytes(hex_hash(authority.as_ref()).0),
			Value::from_bytes(hex_hash(payout.as_ref()).0),
			Value::u128(nonce.into()),
			Value::from_bytes(AsRef::<[u8]>::as_ref(&signature)),
		],
	);
	api.tx()
		.sign_and_submit_then_watch_default(&register, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	let claim = subxt::dynamic::tx(
		"MfcRewards",
		"claim_validator_credit",
		vec![Value::u128(target.into()), Value::from_bytes(hex_hash(authority.as_ref()).0)],
	);
	api.tx()
		.sign_and_submit_then_watch_default(&claim, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	let totals = http.request::<serde_json::Value, _>("mfc_rewardTotals", rpc_params![Option::<H256>::None]).await?;
	let paid = totals["validatorPaid"].as_str().ok_or("Missing validator paid total")?.parse::<u128>()?;
	assert!(paid >= amount.parse::<u128>()?);
	assert_eq!(
		totals["validatorCredit"].as_str().ok_or("Missing remaining credit")?.parse::<u128>()?,
		initial_credit - amount.parse::<u128>()?
	);
	println!("validator_credit_claimed=true paid={amount}");

	let replay = api
		.tx()
		.sign_and_submit_then_watch_default(&release, &alice)
		.await?
		.wait_for_finalized_success()
		.await;
	assert!(replay.is_err(), "Duplicate validator release was accepted");
	println!("duplicate_release_rejected=true");
	Ok(())
}

use jsonrpsee::{core::client::ClientT, http_client::HttpClientBuilder, rpc_params};
use pallet_mfc_rewards::validator_payout_payload;
use sp_consensus_grandpa::AuthorityId;
use sp_core::{crypto::{AccountId32, Ss58Codec}, ed25519, Pair, H256};
use std::str::FromStr;
use subxt::{dynamic::Value, OnlineClient, PolkadotConfig};
use subxt_signer::{sr25519, SecretUri};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let url = std::env::var("DEVNET_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:9944".to_string());
	let local = url.starts_with("ws://127.0.0.1:") || url.starts_with("ws://localhost:");
	let (authority_pair, payout, relayer) = if let Ok(validator_seed) = std::env::var("MFC_VALIDATOR_SEED") {
		let relayer_seed = std::env::var("MFC_RELAYER_SEED")
			.map_err(|_| "MFC_RELAYER_SEED is required with MFC_VALIDATOR_SEED")?;
		let payout = std::env::var("MFC_VALIDATOR_PAYOUT")
			.map_err(|_| "MFC_VALIDATOR_PAYOUT is required with MFC_VALIDATOR_SEED")?;
		(
			ed25519::Pair::from_string(&validator_seed, None)?,
			AccountId32::from_ss58check(&payout)?,
			sr25519::Keypair::from_uri(&SecretUri::from_str(&relayer_seed)?)?,
		)
	} else if local {
		(
			ed25519::Pair::from_string("//Alice", None)?,
			sr25519::dev::alice().public_key().0.into(),
			sr25519::dev::alice(),
		)
	} else {
		return Err("MFC_VALIDATOR_SEED is required outside the development network".into());
	};
	let set_id = std::env::var("MFC_VALIDATOR_SET_ID")
		.ok()
		.map(|value| value.parse::<u64>())
		.transpose()?
		.unwrap_or(0);
	let authority: AuthorityId = authority_pair.public().into();
	let payout: AccountId32 = payout;
	let api = OnlineClient::<PolkadotConfig>::from_insecure_url(&url).await?;
	let http = HttpClientBuilder::default().build(url.replace("ws://", "http://"))?;
	let nonce = http
		.request::<String, _>(
			"mfc_validatorPayoutNonce",
			rpc_params![set_id, H256::from_slice(authority.as_ref()), Option::<H256>::None],
		)
		.await?
		.parse::<u64>()?;
	let genesis = http.request::<H256, _>("chain_getBlockHash", rpc_params![0]).await?;
	let payload = validator_payout_payload(genesis, set_id, &authority, &payout, nonce);
	let signature = authority_pair.sign(&payload);
	let register = subxt::dynamic::tx(
		"MfcRewards",
		"register_validator_payout",
		vec![
			Value::u128(set_id.into()),
			Value::from_bytes(H256::from_slice(authority.as_ref()).0),
			Value::from_bytes(H256::from_slice(payout.as_ref()).0),
			Value::u128(nonce.into()),
			Value::from_bytes(AsRef::<[u8]>::as_ref(&signature)),
		],
	);
	api.tx()
		.sign_and_submit_then_watch_default(&register, &relayer)
		.await?
		.wait_for_finalized_success()
		.await?;
	println!("validator_payout_registered=true set_id={set_id} nonce={nonce}");
	Ok(())
}

use jsonrpsee::{core::client::ClientT, http_client::HttpClientBuilder, rpc_params};
use sp_consensus_grandpa::AuthorityId;
use sp_core::{ed25519, Pair, H256};
use std::str::FromStr;
use subxt::{dynamic::Value, OnlineClient, PolkadotConfig};
use subxt_signer::{sr25519, SecretUri};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let url = std::env::var("DEVNET_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:9944".to_string());
	let validator_seed = std::env::var("MFC_VALIDATOR_SEED")
		.map_err(|_| "MFC_VALIDATOR_SEED is required")?;
	let relayer_seed = std::env::var("MFC_RELAYER_SEED")
		.map_err(|_| "MFC_RELAYER_SEED is required")?;
	let target = std::env::var("MFC_REWARD_TARGET")
		.map_err(|_| "MFC_REWARD_TARGET is required")?
		.parse::<u32>()?;
	let authority_pair = ed25519::Pair::from_string(&validator_seed, None)?;
	let relayer = sr25519::Keypair::from_uri(&SecretUri::from_str(&relayer_seed)?)?;
	let authority: AuthorityId = authority_pair.public().into();
	let authority_hash = H256::from_slice(authority.as_ref());
	let api = OnlineClient::<PolkadotConfig>::from_insecure_url(&url).await?;
	let http = HttpClientBuilder::default().build(url.replace("ws://", "http://"))?;
	let credit = http
		.request::<Option<String>, _>(
			"mfc_validatorCredit",
			rpc_params![target, authority_hash, Option::<H256>::None],
		)
		.await?
		.ok_or("No released validator credit for this target")?;
	let claim = subxt::dynamic::tx(
		"MfcRewards",
		"claim_validator_credit",
		vec![Value::u128(target.into()), Value::from_bytes(authority_hash.0)],
	);
	api.tx()
		.sign_and_submit_then_watch_default(&claim, &relayer)
		.await?
		.wait_for_finalized_success()
		.await?;
	println!("validator_credit_claimed=true target={target} amount={credit}");
	Ok(())
}

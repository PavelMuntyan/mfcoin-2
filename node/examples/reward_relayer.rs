use codec::DecodeAll;
use jsonrpsee::{core::client::ClientT, http_client::HttpClientBuilder, rpc_params};
use serde::{Deserialize, Serialize};
use sp_core::H256;
use std::{path::{Path, PathBuf}, str::FromStr};
use subxt::{dynamic::Value, OnlineClient, PolkadotConfig};
use subxt_signer::{sr25519::{self, Keypair}, SecretUri};

const MAX_PENDING: usize = 1200;

fn proof_counts(proof: &[u8]) -> Result<(u32, u32), Box<dyn std::error::Error>> {
	let justification = sc_consensus_grandpa::GrandpaJustification::<
		solochain_template_runtime::opaque::Block,
	>::decode_all(&mut &proof[..])?;
	Ok((
		justification.justification.commit.precommits.len() as u32,
		justification.justification.votes_ancestries.len() as u32,
	))
}

#[derive(Default, Serialize, Deserialize)]
struct Queue {
	next_target: u32,
	pending: Vec<PendingProof>,
	#[serde(default)]
	bootstrap_complete: bool,
}

#[derive(Serialize, Deserialize)]
struct PendingProof {
	target: u32,
	proof: Vec<u8>,
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

fn load_queue(path: &Path) -> Result<Queue, Box<dyn std::error::Error>> {
	if !path.exists() {
		return Ok(Queue::default());
	}
	Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn save_queue(path: &Path, queue: &Queue) -> Result<(), Box<dyn std::error::Error>> {
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)?;
	}
	let temporary = path.with_extension("tmp");
	std::fs::write(&temporary, serde_json::to_vec(queue)?)?;
	std::fs::rename(temporary, path)?;
	Ok(())
}

async fn reward(
	http: &jsonrpsee::http_client::HttpClient,
	target: u32,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
	Ok(http
		.request("mfc_validatorReward", rpc_params![target, Option::<H256>::None])
		.await?)
}

async fn submit_release(
	api: &OnlineClient<PolkadotConfig>,
	http: &jsonrpsee::http_client::HttpClient,
	signer: &Keypair,
	target: u32,
	proof: Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
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
	let result = api
		.tx()
		.sign_and_submit_then_watch_default(&release, signer)
		.await?
		.wait_for_finalized_success()
		.await;
	if let Err(error) = result {
		let obligation = reward(http, target).await?;
		if obligation.is_null() {
			println!("validator_relay_expired_skipped=true target={target}");
			return Ok(());
		}
		if obligation["released"].is_null() {
			return Err(format!("Release failed for target {target}: {error}").into());
		}
	}
	println!("validator_relay_finalized=true target={target}");
	Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let url = std::env::var("DEVNET_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:9944".to_string());
	let local = url.starts_with("ws://127.0.0.1:") || url.starts_with("ws://localhost:");
	let signer = if let Ok(seed) = std::env::var("MFC_RELAYER_SEED") {
		let uri = SecretUri::from_str(&seed)?;
		Keypair::from_uri(&uri)?
	} else if local {
		sr25519::dev::alice()
	} else {
		return Err("MFC_RELAYER_SEED is required outside the development network".into());
	};
	let queue_path = match std::env::var("MFC_RELAYER_QUEUE") {
		Ok(path) => PathBuf::from(path),
		Err(_) if local => PathBuf::from(".devnet/reward-relayer-queue.json"),
		Err(_) => return Err("MFC_RELAYER_QUEUE is required outside the development network".into()),
	};
	let start = std::env::var("MFC_RELAYER_START_TARGET")
		.ok()
		.map(|value| value.parse::<u32>())
		.transpose()?
		.unwrap_or(1);
	let end = std::env::var("MFC_RELAYER_END_TARGET")
		.ok()
		.map(|value| value.parse::<u32>())
		.transpose()?;
	let once = std::env::var("MFC_RELAYER_ONCE").unwrap_or_default() == "1";
	let mut once_mature = None;
	let api = OnlineClient::<PolkadotConfig>::from_insecure_url(&url).await?;
	let http = HttpClientBuilder::default().build(url.replace("ws://", "http://"))?;
	let mut queue = load_queue(&queue_path)?;
	if queue.next_target == 0 {
		queue.next_target = start;
	}

	loop {
		for index in 0..queue.pending.len() {
			let pending = &queue.pending[index];
			if reward(&http, pending.target).await?.is_null() {
				println!("validator_relay_expired_skipped=true target={}", pending.target);
				continue;
			}
			submit_release(&api, &http, &signer, pending.target, pending.proof.clone()).await?;
		}
		queue.pending.clear();
		save_queue(&queue_path, &queue)?;

		let finalized_hash = http.request::<H256, _>("chain_getFinalizedHead", rpc_params![]).await?;
		let header = http
			.request::<serde_json::Value, _>("chain_getHeader", rpc_params![finalized_hash])
			.await?;
		let finalized = u32::from_str_radix(
			header["number"].as_str().ok_or("Missing finalized number")?.trim_start_matches("0x"),
			16,
		)?;
		let current_mature = end.map_or_else(
			|| finalized.saturating_sub(10),
			|target| target.min(finalized.saturating_sub(10)),
		);
		let mature = if once { *once_mature.get_or_insert(current_mature) } else { current_mature };
		while queue.next_target <= mature {
			let target = queue.next_target;
			let obligation = reward(&http, target).await?;
			if obligation.is_null() || !obligation["released"].is_null() {
				queue.next_target += 1;
				save_queue(&queue_path, &queue)?;
				continue;
			}
			let target_hash = http.request::<H256, _>("chain_getBlockHash", rpc_params![target]).await?;
			let block = http
				.request::<serde_json::Value, _>("chain_getBlock", rpc_params![target_hash])
				.await?;
			let proof = match justification_bytes(&block) {
				Ok(proof) => proof,
				Err(error) => {
					let reason = error.to_string();
					if reason == "Missing justifications" || reason == "No FRNK justification in target block" {
						println!("validator_relay_unfinalized_skipped=true target={target}");
						queue.next_target += 1;
						save_queue(&queue_path, &queue)?;
						continue;
					}
					return Err(format!("Cannot release target {target}: {reason}").into());
				}
			};
			queue.bootstrap_complete = true;
			if queue.pending.len() >= MAX_PENDING {
				return Err("Reward relayer queue is full".into());
			}
			queue.pending.push(PendingProof { target, proof: proof.clone() });
			save_queue(&queue_path, &queue)?;
			submit_release(&api, &http, &signer, target, proof).await?;
			queue.pending.retain(|pending| pending.target != target);
			queue.next_target += 1;
			save_queue(&queue_path, &queue)?;
		}
		if once {
			return Ok(());
		}
		tokio::time::sleep(std::time::Duration::from_secs(3)).await;
	}
}

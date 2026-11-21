use jsonrpsee::{core::client::ClientT, http_client::HttpClientBuilder, rpc_params};
use pallet_freeland_records::{verify_registry_proof, RegistryProof};
use sp_core::H256;
use subxt::{config::DefaultExtrinsicParamsBuilder, dynamic::Value, tx::TxStatus, OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let url = std::env::var("DEVNET_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:9944".to_string());
	if !url.starts_with("ws://127.0.0.1:") && !url.starts_with("ws://localhost:") {
		return Err("This development example only connects to a local devnet".into());
	}

	let api = OnlineClient::<PolkadotConfig>::from_insecure_url(&url).await?;
	let wait_for_finality = std::env::var("FREELAND_WAIT_FOR_FINALITY").unwrap_or_default() != "0";
	let alice = dev::alice();
	let set_issuer = Value::unnamed_variant(
		"FreelandRecords",
		[Value::unnamed_variant(
			"set_issuer",
			[
				Value::unnamed_variant("Passport", []),
				Value::from_bytes(alice.public_key().0),
				Value::bool(true),
			],
		)],
	);
	let sudo = subxt::dynamic::tx("Sudo", "sudo", vec![set_issuer]);
	if wait_for_finality {
		let events = api
			.tx()
			.sign_and_submit_then_watch_default(&sudo, &alice)
			.await?
			.wait_for_finalized_success()
			.await?;
		println!("issuer_update_finalized={:#x}", events.extrinsic_hash());
	} else {
		let mut progress = api.tx().sign_and_submit_then_watch_default(&sudo, &alice).await?;
		while let Some(status) = progress.next().await {
			match status? {
				TxStatus::InBestBlock(details) => {
					println!("issuer_update_in_block={:#x}", details.extrinsic_hash());
					break;
				},
				TxStatus::Error { message } => return Err(format!("issuer update failed: {message}").into()),
				TxStatus::Invalid { message } => return Err(format!("issuer update invalid: {message}").into()),
				TxStatus::Dropped { message } => return Err(format!("issuer update dropped: {message}").into()),
				_ => {},
			}
		}
	}

	let subject = H256([0x11; 32]);
	let record = subxt::dynamic::tx(
		"FreelandRecords",
		"submit_record",
		vec![
			Value::unnamed_variant("Passport", []),
			Value::unnamed_variant("Issue", []),
			Value::from_bytes(subject.0),
			Value::from_bytes([0x22; 32]),
			Value::u128(0),
			Value::from_bytes([0x33; 32]),
		],
	);
	if wait_for_finality {
		let events = api
			.tx()
			.sign_and_submit_then_watch_default(&record, &alice)
			.await?
			.wait_for_finalized_success()
			.await?;
		println!("record_finalized={:#x}", events.extrinsic_hash());
	} else {
		let params = DefaultExtrinsicParamsBuilder::default().nonce(1).build();
		let mut transaction = api.tx().create_partial_offline(&record, params)?;
		let hash = transaction.sign(&alice).submit().await?;
		println!("record_submitted={hash:#x}");
	}
	println!("subject_commitment={subject:#x}");
	if !wait_for_finality {
		return Ok(());
	}

	let http = HttpClientBuilder::default().build(url.replace("ws://", "http://"))?;
	let finalized = http.request::<H256, _>("chain_getFinalizedHead", rpc_params![]).await?;
	let proof = http
		.request::<RegistryProof, _>(
			"freeland_registryProof",
			rpc_params!["passport", subject, finalized],
		)
		.await?;
	if !verify_registry_proof(&proof) {
		return Err("Registry proof verification failed".into());
	}
	let info = http
		.request::<serde_json::Value, _>("freeland_latestBlockInfo", rpc_params![finalized])
		.await?;
	let expected_root = info["registryRoot"].as_str().ok_or("Missing registryRoot")?;
	if format!("{:#x}", proof.root) != expected_root {
		return Err("Proof root does not match FreelandBlockInfo".into());
	}
	println!("proof_verified=true siblings={} root={expected_root}", proof.siblings.len());

	Ok(())
}

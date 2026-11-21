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
	let http = HttpClientBuilder::default().build(url.replace("ws://", "http://"))?;
	let wait_for_finality = std::env::var("FREELAND_WAIT_FOR_FINALITY").unwrap_or_default() != "0";
	let alice = dev::alice();
	let bob = dev::bob();
	let charlie = dev::charlie();
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
	let as_multi = subxt::dynamic::tx(
		"Multisig",
		"as_multi",
		vec![
			Value::u128(2),
			Value::unnamed_composite([
					Value::from_bytes(bob.public_key().0),
					Value::from_bytes(charlie.public_key().0),
				]),
			Value::unnamed_variant("None", []),
			set_issuer,
			Value::named_composite([
				("ref_time".to_string(), Value::u128(100_000_000_000)),
				("proof_size".to_string(), Value::u128(1_000_000)),
			]),
		],
	);
	let timepoint = if wait_for_finality {
		let in_block = api
			.tx()
			.sign_and_submit_then_watch_default(&as_multi, &alice)
			.await?
			.wait_for_finalized()
			.await?;
		let events = in_block.wait_for_success().await?;
		println!("multisig_finalized={:#x}", events.extrinsic_hash());
		(in_block.block_hash(), events.extrinsic_index())
	} else {
		let mut progress = api.tx().sign_and_submit_then_watch_default(&as_multi, &alice).await?;
		let mut timepoint = None;
		while let Some(status) = progress.next().await {
			match status? {
				TxStatus::InBestBlock(details) => {
					let events = details.wait_for_success().await?;
					println!("multisig_in_block={:#x}", events.extrinsic_hash());
					timepoint = Some((details.block_hash(), events.extrinsic_index()));
					break;
				},
				TxStatus::Error { message } => return Err(format!("multisig failed: {message}").into()),
				TxStatus::Invalid { message } => return Err(format!("multisig invalid: {message}").into()),
				TxStatus::Dropped { message } => return Err(format!("multisig dropped: {message}").into()),
				_ => {},
			}
		}
		timepoint.ok_or("Multisig was not included")?
	};
	let header = http
		.request::<serde_json::Value, _>("chain_getHeader", rpc_params![timepoint.0])
		.await?;
	let height = u64::from_str_radix(header["number"].as_str().ok_or("Missing block number")?.trim_start_matches("0x"), 16)?;
	let maybe_timepoint = Value::unnamed_variant(
		"Some",
		[Value::named_composite([
			("height".to_string(), Value::u128(height.into())),
			("index".to_string(), Value::u128(timepoint.1.into())),
		])],
	);
	let second_approval = subxt::dynamic::tx(
		"Multisig",
		"as_multi",
		vec![
			Value::u128(2),
			Value::unnamed_composite([
				Value::from_bytes(bob.public_key().0),
				Value::from_bytes(alice.public_key().0),
			]),
			maybe_timepoint,
			Value::unnamed_variant(
				"FreelandRecords",
				[Value::unnamed_variant(
					"set_issuer",
					[
						Value::unnamed_variant("Passport", []),
						Value::from_bytes(alice.public_key().0),
						Value::bool(true),
					],
				)],
			),
			Value::named_composite([
				("ref_time".to_string(), Value::u128(100_000_000_000)),
				("proof_size".to_string(), Value::u128(1_000_000)),
			]),
		],
	);
	if wait_for_finality {
		let events = api
			.tx()
			.sign_and_submit_then_watch_default(&second_approval, &charlie)
			.await?
			.wait_for_finalized_success()
			.await?;
		println!("second_approval_finalized={:#x}", events.extrinsic_hash());
	} else {
		let mut progress = api.tx().sign_and_submit_then_watch_default(&second_approval, &charlie).await?;
		while let Some(status) = progress.next().await {
			match status? {
				TxStatus::InBestBlock(details) => {
					println!("second_approval_in_block={:#x}", details.extrinsic_hash());
					break;
				},
				TxStatus::Error { message } => return Err(format!("second approval failed: {message}").into()),
				TxStatus::Invalid { message } => return Err(format!("second approval invalid: {message}").into()),
				TxStatus::Dropped { message } => return Err(format!("second approval dropped: {message}").into()),
				_ => {},
			}
		}
	}

	let pending = http
		.request::<Vec<serde_json::Value>, _>("freeland_pendingIssuerChanges", rpc_params![Option::<H256>::None])
		.await?;
	let pending = pending.first().ok_or("No pending issuer change")?;
	let proposal_id: H256 = pending["proposalId"].as_str().ok_or("Missing proposalId")?.parse()?;
	let execute_at = pending["executeAt"].as_u64().ok_or("Missing executeAt")?;
	while u64::from_str_radix(
		http.request::<serde_json::Value, _>("chain_getHeader", rpc_params![])
			.await?["number"]
			.as_str()
			.ok_or("Missing block number")?
			.trim_start_matches("0x"),
		16,
	)? < execute_at
	{
		tokio::time::sleep(std::time::Duration::from_millis(500)).await;
	}

	let execute = subxt::dynamic::tx(
		"FreelandRecords",
		"execute_issuer_change",
		vec![Value::from_bytes(proposal_id.0)],
	);
	if wait_for_finality {
		let events = api
			.tx()
			.sign_and_submit_then_watch_default(&execute, &alice)
			.await?
			.wait_for_finalized_success()
			.await?;
		println!("issuer_change_executed={:#x}", events.extrinsic_hash());
	} else {
		let params = DefaultExtrinsicParamsBuilder::default().nonce(1).build();
		let mut transaction = api.tx().create_partial_offline(&execute, params)?;
		let hash = transaction.sign(&alice).submit().await?;
		println!("issuer_change_submitted={hash:#x}");
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
		let params = DefaultExtrinsicParamsBuilder::default().nonce(2).build();
		let mut transaction = api.tx().create_partial_offline(&record, params)?;
		let hash = transaction.sign(&alice).submit().await?;
		println!("record_submitted={hash:#x}");
	}
	println!("subject_commitment={subject:#x}");
	if !wait_for_finality {
		return Ok(());
	}

	let finalized = http.request::<H256, _>("chain_getFinalizedHead", rpc_params![]).await?;
	let proof = http
		.request::<RegistryProof, _>("freeland_registryProof", rpc_params!["passport", subject, finalized])
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

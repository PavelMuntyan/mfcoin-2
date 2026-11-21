use subxt::{dynamic::Value, OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::dev;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let url = std::env::var("DEVNET_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:9944".to_string());
	if !url.starts_with("ws://127.0.0.1:") && !url.starts_with("ws://localhost:") {
		return Err("This development example only connects to a local devnet".into());
	}

	let api = OnlineClient::<PolkadotConfig>::from_insecure_url(&url).await?;
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
	let issuer_events = api
		.tx()
		.sign_and_submit_then_watch_default(&sudo, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	println!("issuer_update_finalized={:#x}", issuer_events.extrinsic_hash());

	let record = subxt::dynamic::tx(
		"FreelandRecords",
		"submit_record",
		vec![
			Value::unnamed_variant("Passport", []),
			Value::unnamed_variant("Issue", []),
			Value::from_bytes([0x11; 32]),
			Value::from_bytes([0x22; 32]),
			Value::u128(0),
			Value::from_bytes([0x33; 32]),
		],
	);
	let record_events = api
		.tx()
		.sign_and_submit_then_watch_default(&record, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;
	println!("record_finalized={:#x}", record_events.extrinsic_hash());
	println!("subject_commitment=0x{}", hex_string(&[0x11; 32]));

	Ok(())
}

fn hex_string(bytes: &[u8]) -> String {
	bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

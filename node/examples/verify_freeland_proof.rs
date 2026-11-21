use jsonrpsee::{core::client::ClientT, http_client::HttpClientBuilder, rpc_params};
use pallet_freeland_records::{verify_registry_proof, RegistryProof};
use sp_core::H256;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let url = std::env::var("DEVNET_HTTP_URL").unwrap_or_else(|_| "http://127.0.0.1:9944".to_string());
	if !url.starts_with("http://127.0.0.1:") && !url.starts_with("http://localhost:") {
		return Err("This development example only connects to a local devnet".into());
	}
	let kind = std::env::var("FREELAND_KIND").unwrap_or_else(|_| "passport".to_string());
	let subject: H256 = std::env::var("FREELAND_SUBJECT")?.parse()?;
	let at = std::env::var("FREELAND_AT")?.parse::<H256>()?;
	let client = HttpClientBuilder::default().build(url)?;
	let proof = client
		.request::<RegistryProof, _>("freeland_registryProof", rpc_params![kind, subject, at])
		.await?;
	if !verify_registry_proof(&proof) {
		return Err("Registry proof verification failed".into());
	}
	println!("proof_verified=true siblings={} root={:#x}", proof.siblings.len(), proof.root);
	Ok(())
}

use serde_json::json;
use sp_core::{crypto::AccountId32, ed25519, sr25519, Pair};
use std::path::{Path, PathBuf};

fn account_hex(account: &AccountId32) -> String {
	let mut encoded = String::from("0x");
	for byte in AsRef::<[u8]>::as_ref(account) {
		encoded.push_str(&format!("{byte:02x}"));
	}
	encoded
}

fn write_secret(
	directory: &Path,
	name: &str,
	crypto: &str,
	secret_phrase: &str,
	public: &AccountId32,
) -> Result<(), Box<dyn std::error::Error>> {
	let path = directory.join(format!("{name}.json"));
	let data = json!({
		"cryptography": crypto,
		"secretPhrase": secret_phrase,
		"publicKey": public,
		"publicKeyHex": account_hex(public),
	});
	let mut options = std::fs::OpenOptions::new();
	options.write(true).create_new(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.mode(0o600);
	}
	let file = options.open(path)?;
	serde_json::to_writer_pretty(file, &data)?;
	Ok(())
}

fn new_sr(role: &str, directory: &Path) -> Result<AccountId32, Box<dyn std::error::Error>> {
	let (pair, phrase, _) = sr25519::Pair::generate_with_phrase(None);
	let account = AccountId32::from(pair.public().0);
	write_secret(directory, role, "sr25519", &phrase, &account)?;
	Ok(account)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let directory = std::env::args_os()
		.nth(1)
		.map(PathBuf::from)
		.unwrap_or_else(|| PathBuf::from(".testnet-secrets"));
	if let Some(parent) = directory.parent() {
		if !parent.as_os_str().is_empty() {
			std::fs::create_dir_all(parent)?;
		}
	}
	std::fs::create_dir(&directory)?;
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
	}

	let mut validators = Vec::new();
	for index in 1..=4 {
		let (pair, phrase, _) = ed25519::Pair::generate_with_phrase(None);
		let account = AccountId32::from(pair.public().0);
		write_secret(
			&directory,
			&format!("validator-{index}"),
			"ed25519",
			&phrase,
			&account,
		)?;
		validators.push(account);
	}
	let mut payout_accounts = Vec::new();
	for index in 1..=4 {
		payout_accounts.push(new_sr(&format!("payout-{index}"), &directory)?);
	}
	let mut governance = (1..=3)
		.map(|index| new_sr(&format!("governance-{index}"), &directory))
		.collect::<Result<Vec<_>, _>>()?;
	governance.sort();
	let relayers = (1..=2)
		.map(|index| new_sr(&format!("relayer-{index}"), &directory))
		.collect::<Result<Vec<_>, _>>()?;
	let treasury = new_sr("treasury", &directory)?;
	let endowed = governance
		.iter()
		.chain(relayers.iter())
		.chain([&treasury])
		.map(|account| (account.clone(), 1_000_000_000_000_000_000u128))
		.chain(payout_accounts.iter().map(|account| (account.clone(), 100_000_000_000_000_000u128)))
		.collect::<Vec<_>>();
	let operators = json!({
		"network": {
			"name": "MFCoin Testnet 112",
			"id": "mfcoin-testnet-112"
		},
		"rewards": {
			"activationBlock": 100,
			"treasuryAccount": treasury
		},
		"validators": validators,
		"governanceSignatories": governance,
		"endowedAccounts": endowed,
	});
	let operators_path = directory.join("operators.json");
	let mut options = std::fs::OpenOptions::new();
	options.write(true).create_new(true);
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.mode(0o644);
	}
	let file = options.open(&operators_path)?;
	serde_json::to_writer_pretty(file, &operators)?;
	println!("Public operators config: {}", operators_path.display());
	Ok(())
}

use solochain_template_runtime::AccountId;
use std::str::FromStr;

fn parse_pow_payout_account(value: &str) -> Result<AccountId, String> {
	if value.starts_with("mfcx1") || value.starts_with("tmfcx1") {
		return mfcoin_address::decode_address(value)
			.map(|decoded| AccountId::new(decoded.account_id))
			.map_err(|error| error.to_string());
	}
	AccountId::from_str(value).map_err(|error| error.to_string())
}

#[derive(Debug, clap::Parser)]
pub struct Cli {
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[clap(flatten)]
	pub run: sc_cli::RunCmd,

	/// Participate in GRANDPA without authoring PoW blocks.
	#[arg(long)]
	pub disable_pow_mining: bool,

	/// Public account receiving PoW rewards in mfcx/tmfcx, SS58 or 0x hex form. No private key is required on the node.
	#[arg(long, value_parser = parse_pow_payout_account)]
	pub pow_payout_account: Option<AccountId>,

	/// Override the number of 64 KiB pages allocated to the runtime.
	#[arg(long)]
	pub runtime_heap_pages: Option<u64>,
}

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Subcommand {
	/// Key management cli utilities
	#[command(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Build a chain specification.
	/// DEPRECATED: `build-spec` command will be removed after 1/04/2026. Use `export-chain-spec`
	/// command instead.
	#[deprecated(
		note = "build-spec command will be removed after 1/04/2026. Use export-chain-spec command instead"
	)]
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Export the chain specification.
	ExportChainSpec(sc_cli::ExportChainSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// Sub-commands concerned with benchmarking.
	#[command(subcommand)]
	Benchmark(frame_benchmarking_cli::BenchmarkCmd),

	/// Db meta columns information.
	ChainInfo(sc_cli::ChainInfoCmd),
}

#[cfg(test)]
mod tests {
	use super::*;

	const MFCX: &str = "mfcx1qr0q6mcy33zau35pvz8qnt87z02mzmrp04ehd7n0d4m67rg8c4axwm22jed";
	const TMFCX: &str = "tmfcx1qr0q6mcy33zau35pvz8qnt87z02mzmrp04ehd7n0d4m67rg8c4axwyt737x";
	const HEX: &str = "0xde0d6f048c45de4681608e09acfe13d5b16c617d7376fa6f6d77af0d07c57a67";

	#[test]
	fn payout_account_accepts_canonical_addresses_and_hex() {
		let expected = AccountId::from_str(HEX).unwrap();
		assert_eq!(parse_pow_payout_account(MFCX), Ok(expected.clone()));
		assert_eq!(parse_pow_payout_account(TMFCX), Ok(expected.clone()));
		assert_eq!(parse_pow_payout_account(HEX), Ok(expected));
	}

	#[test]
	fn payout_account_preserves_ss58_compatibility() {
		let ss58 = "5CJMq1EeE8e4Px2NWCr9hUmxKXAeCqzPnfKSNeQpQZLUm6jR";
		assert_eq!(parse_pow_payout_account(ss58), AccountId::from_str(ss58).map_err(|error| error.to_string()));
	}

	#[test]
	fn payout_account_rejects_an_invalid_canonical_checksum() {
		let mut invalid = MFCX.to_string();
		invalid.pop();
		invalid.push('q');
		assert!(parse_pow_payout_account(&invalid).is_err());
		assert!(parse_pow_payout_account(&MFCX.replacen("mfcx", "mf0x", 1)).is_err());
		assert!(parse_pow_payout_account("mfcx1qqqqqq").is_err());
	}
}

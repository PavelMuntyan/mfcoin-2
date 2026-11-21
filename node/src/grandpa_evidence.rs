use codec::DecodeAll;
use sc_consensus_grandpa::GrandpaJustification;
use sp_consensus_grandpa::AuthorityList;
use sp_runtime::traits::Header as HeaderT;
use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq)]
pub struct ValidatorEvidence {
	pub block_hash: String,
	pub block_number: u32,
	pub round: u64,
	pub precommit_authorities: BTreeSet<String>,
}

pub fn verify_signed_block(
	signed_block: sp_runtime::generic::SignedBlock<solochain_template_runtime::opaque::Block>,
	authorities: &AuthorityList,
	set_id: u64,
) -> Result<ValidatorEvidence, String> {
	let block_hash = signed_block.block.header.hash();
	let block_number = *signed_block.block.header.number();
	let justifications = signed_block
		.justifications
		.ok_or_else(|| "No GRANDPA justifications attached".to_string())?;
	let encoded = justifications
		.get(*b"FRNK")
		.ok_or_else(|| "No GRANDPA justification attached".to_string())?;
	let justification =
		GrandpaJustification::<solochain_template_runtime::opaque::Block>::decode_all(
			&mut &encoded[..],
		)
		.map_err(|_| "Could not decode GRANDPA justification".to_string())?;
	if (
		justification.justification.commit.target_hash,
		justification.justification.commit.target_number,
	) != (block_hash, block_number)
	{
		return Err("GRANDPA commit target does not match the requested block".to_string());
	}
	justification
		.verify(set_id, &authorities)
		.map_err(|error| error.to_string())?;
	let precommit_authorities = justification
		.justification
		.commit
		.precommits
		.iter()
		.map(|signed| signed.id.to_string())
		.collect();
	Ok(ValidatorEvidence {
		block_hash: format!("{block_hash:#x}"),
		block_number,
		round: justification.justification.round,
		precommit_authorities,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_keyring::Ed25519Keyring;

	#[test]
	fn missing_justification_is_rejected() {
		let signed_block = sp_runtime::generic::SignedBlock {
			block: sp_runtime::generic::Block {
				header: sp_runtime::generic::Header {
					number: 1,
					parent_hash: Default::default(),
					state_root: Default::default(),
					extrinsics_root: Default::default(),
					digest: Default::default(),
				},
				extrinsics: vec![],
			},
			justifications: None,
		};
		let authorities = vec![(Ed25519Keyring::Alice.public().into(), 1)];
		let error = verify_signed_block(signed_block, &authorities, 0).unwrap_err();
		assert_eq!(error, "No GRANDPA justifications attached");
	}
}

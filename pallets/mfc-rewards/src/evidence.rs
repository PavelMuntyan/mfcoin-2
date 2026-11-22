use crate::{pallet::Error, Config, HeaderOf};
use alloc::{
	collections::{BTreeMap, BTreeSet},
	vec::Vec,
};
use finality_grandpa::{voter_set::VoterSet, Chain};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_consensus_grandpa::{
	check_message_signature, AuthorityId, AuthorityList, GrandpaJustification,
};
use sp_runtime::traits::Header as HeaderT;

struct AncestryChain<T: Config>
where
	BlockNumberFor<T>: finality_grandpa::BlockNumberOps,
{
	ancestry: BTreeMap<T::Hash, HeaderOf<T>>,
}

impl<T: Config> AncestryChain<T>
where
	BlockNumberFor<T>: finality_grandpa::BlockNumberOps,
{
	fn new(headers: &[HeaderOf<T>]) -> Self {
		let ancestry = headers.iter().cloned().map(|header| (header.hash(), header)).collect();
		Self { ancestry }
	}
}

impl<T: Config> Chain<T::Hash, BlockNumberFor<T>> for AncestryChain<T>
where
	BlockNumberFor<T>: finality_grandpa::BlockNumberOps,
{
	fn ancestry(
		&self,
		base: T::Hash,
		block: T::Hash,
	) -> Result<Vec<T::Hash>, finality_grandpa::Error> {
		let mut route = Vec::new();
		let mut current = block;
		for _ in 0..=self.ancestry.len() {
			if current == base {
				route.pop();
				return Ok(route);
			}
			let header = self.ancestry.get(&current).ok_or(finality_grandpa::Error::NotDescendent)?;
			current = *header.parent_hash();
			route.push(current);
		}
		Err(finality_grandpa::Error::NotDescendent)
	}
}

pub fn verify_justification<T: Config>(
	justification: &GrandpaJustification<HeaderOf<T>>,
	set_id: u64,
	authorities: &AuthorityList,
) -> Result<Vec<(AuthorityId, u64)>, Error<T>>
where
	BlockNumberFor<T>: finality_grandpa::BlockNumberOps,
{
	let commit = &justification.commit;
	let voters = VoterSet::new(authorities.iter().cloned()).ok_or(Error::<T>::InvalidValidatorSet)?;
	let chain = AncestryChain::<T>::new(&justification.votes_ancestries);
	let mut seen = BTreeSet::new();
	let mut signers = Vec::new();

	for signed in &commit.precommits {
		let weight = authorities
			.iter()
			.find(|(id, _)| id == &signed.id.clone())
			.map(|(_, weight)| *weight)
			.ok_or(Error::<T>::UnknownValidator)?;
		if !seen.insert(signed.id.clone()) {
			return Err(Error::<T>::DuplicateValidator.into());
		}
		let message = finality_grandpa::Message::Precommit(signed.precommit.clone());
		if !check_message_signature(
			&message,
			&signed.id.clone(),
			&signed.signature,
			justification.round,
			set_id,
		)
		.is_valid()
		{
			return Err(Error::<T>::InvalidValidatorSignature.into());
		}
		if signed.precommit.target_number < commit.target_number {
			return Err(Error::<T>::InvalidVoteAncestry.into());
		}
		chain
			.ancestry(commit.target_hash, signed.precommit.target_hash)
			.map_err(|_| Error::<T>::InvalidVoteAncestry)?;
		signers.push((signed.id.clone(), weight));
	}

	let result = finality_grandpa::validate_commit(commit, &voters, &chain)
		.map_err(|_| Error::<T>::InvalidJustification)?;
	if !result.is_valid() {
		return Err(Error::<T>::InvalidJustification.into());
	}

	let base = commit
		.precommits
		.iter()
		.map(|signed| &signed.precommit)
		.min_by_key(|precommit| precommit.target_number)
		.map(|precommit| precommit.target_hash)
		.ok_or(Error::<T>::InvalidJustification)?;
	let mut visited = BTreeSet::new();
	for signed in &commit.precommits {
		if signed.precommit.target_hash == base {
			continue;
		}
		let route = chain
			.ancestry(base, signed.precommit.target_hash)
			.map_err(|_| Error::<T>::InvalidVoteAncestry)?;
		visited.insert(signed.precommit.target_hash);
		visited.extend(route);
	}
	if visited != chain.ancestry.keys().copied().collect() {
		return Err(Error::<T>::UnusedAncestryHeaders.into());
	}

	Ok(signers)
}

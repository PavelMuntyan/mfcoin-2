use super::RecordKind;
use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::{RuntimeDebug, TypeInfo};
use sp_core::H256;

pub const REGISTRY_DEPTH: u16 = 256;

const REGISTRY_KEY_DOMAIN: &[u8] = b"MFC/freeland-smt-key/v1";
const REGISTRY_LEAF_DOMAIN: &[u8] = b"MFC/freeland-smt-leaf/v1";
const REGISTRY_EMPTY_LEAF_DOMAIN: &[u8] = b"MFC/freeland-smt-empty-leaf/v1";
const REGISTRY_NODE_DOMAIN: &[u8] = b"MFC/freeland-smt-node/v1";

#[derive(Clone, Copy, Decode, Encode, Eq, MaxEncodedLen, Ord, PartialEq, PartialOrd, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct RegistryEntry {
	pub kind: RecordKind,
	pub subject_commitment: H256,
	pub record_id: H256,
}

#[derive(Clone, Decode, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize), serde(rename_all = "camelCase"))]
pub struct RegistryProof {
	pub kind: RecordKind,
	pub subject_commitment: H256,
	pub entry: Option<RegistryEntry>,
	pub root: H256,
	pub siblings: Vec<H256>,
}

fn hash_with_domain(domain: &[u8], data: &[u8]) -> H256 {
	let mut payload = Vec::with_capacity(domain.len() + data.len());
	payload.extend_from_slice(domain);
	payload.extend_from_slice(data);
	H256(sp_io::hashing::blake2_256(&payload))
}

pub fn registry_index(kind: RecordKind, subject_commitment: H256) -> H256 {
	hash_with_domain(REGISTRY_KEY_DOMAIN, &(kind, subject_commitment).encode())
}

pub(crate) fn registry_leaf(entry: &RegistryEntry) -> H256 {
	hash_with_domain(REGISTRY_LEAF_DOMAIN, &entry.encode())
}

pub(crate) fn registry_node(level: u16, left: H256, right: H256) -> H256 {
	hash_with_domain(REGISTRY_NODE_DOMAIN, &(level, left, right).encode())
}

fn zero_hashes() -> [H256; REGISTRY_DEPTH as usize + 1] {
	let mut zeros = [H256::zero(); REGISTRY_DEPTH as usize + 1];
	zeros[0] = hash_with_domain(REGISTRY_EMPTY_LEAF_DOMAIN, &[]);
	for level in 1..=REGISTRY_DEPTH as usize {
		zeros[level] = registry_node((level - 1) as u16, zeros[level - 1], zeros[level - 1]);
	}
	zeros
}

pub fn empty_registry_root() -> H256 {
	zero_hashes()[REGISTRY_DEPTH as usize]
}

pub(crate) fn bit(index: &H256, level: u16) -> bool {
	let byte = index.0[31 - usize::from(level / 8)];
	(byte & (1u8 << (level % 8))) != 0
}

fn set_bit(index: &mut H256, level: u16, value: bool) {
	let byte = &mut index.0[31 - usize::from(level / 8)];
	let mask = 1u8 << (level % 8);
	if value {
		*byte |= mask;
	} else {
		*byte &= !mask;
	}
}

pub(crate) fn path_for(index: &H256, level: u16) -> H256 {
	let mut path = *index;
	for bit in 0..level {
		set_bit(&mut path, bit, false);
	}
	path
}

pub fn update_registry_path(
	entry: &RegistryEntry,
	mut read_node: impl FnMut(u16, H256) -> Option<H256>,
	mut write_node: impl FnMut(u16, H256, H256),
) -> H256 {
	let index = registry_index(entry.kind, entry.subject_commitment);
	let zeros = zero_hashes();
	let mut current = registry_leaf(entry);
	write_node(0, index, current);
	for level in 0..REGISTRY_DEPTH {
		let sibling_path = {
			let mut path = path_for(&index, level);
			set_bit(&mut path, level, !bit(&index, level));
			path
		};
		let sibling = read_node(level, sibling_path).unwrap_or(zeros[level as usize]);
		current = if bit(&index, level) {
			registry_node(level, sibling, current)
		} else {
			registry_node(level, current, sibling)
		};
		write_node(level + 1, path_for(&index, level + 1), current);
	}
	current
}

pub fn build_registry_proof(
	kind: RecordKind,
	subject_commitment: H256,
	entry: Option<RegistryEntry>,
	root: H256,
	mut read_node: impl FnMut(u16, H256) -> Option<H256>,
) -> RegistryProof {
	let index = registry_index(kind, subject_commitment);
	let zeros = zero_hashes();
	let siblings = (0..REGISTRY_DEPTH)
		.map(|level| {
			let mut path = path_for(&index, level);
			set_bit(&mut path, level, !bit(&index, level));
			read_node(level, path).unwrap_or(zeros[level as usize])
		})
		.collect();
	RegistryProof { kind, subject_commitment, entry, root, siblings }
}

pub fn verify_registry_proof(proof: &RegistryProof) -> bool {
	if proof.siblings.len() != REGISTRY_DEPTH as usize {
		return false;
	}
	if let Some(entry) = proof.entry {
		if entry.kind != proof.kind || entry.subject_commitment != proof.subject_commitment {
			return false;
		}
	}
	let index = registry_index(proof.kind, proof.subject_commitment);
	let zeros = zero_hashes();
	let mut current = match &proof.entry {
		Some(entry) => registry_leaf(entry),
		None => zeros[0],
	};
	for (level, sibling) in proof.siblings.iter().enumerate() {
		let level = level as u16;
		current = if bit(&index, level) {
			registry_node(level, *sibling, current)
		} else {
			registry_node(level, current, *sibling)
		};
	}
	current == proof.root
}


pub fn registry_nodes_from_entries(entries: &[RegistryEntry]) -> BTreeMap<(u16, H256), H256> {
	let zeros = zero_hashes();
	let mut all_nodes = BTreeMap::new();
	let mut level_nodes = entries
		.iter()
		.map(|entry| (registry_index(entry.kind, entry.subject_commitment), registry_leaf(entry)))
		.collect::<BTreeMap<_, _>>();
	for level in 0..REGISTRY_DEPTH {
		let mut next = BTreeMap::new();
		for (path, current) in &level_nodes {
			all_nodes.insert((level, *path), *current);
			let mut sibling_path = path_for(path, level);
			set_bit(&mut sibling_path, level, !bit(path, level));
			let sibling = level_nodes.get(&sibling_path).copied().unwrap_or(zeros[level as usize]);
			let parent = if bit(path, level) {
				registry_node(level, sibling, *current)
			} else {
				registry_node(level, *current, sibling)
			};
			next.insert(path_for(path, level + 1), parent);
		}
		level_nodes = next;
	}
	for (path, current) in level_nodes {
		all_nodes.insert((REGISTRY_DEPTH, path), current);
	}
	all_nodes
}

pub fn registry_root_from_entries(entries: &[RegistryEntry]) -> H256 {
	let nodes = registry_nodes_from_entries(entries);
	nodes
		.iter()
		.find(|((level, _), _)| *level == REGISTRY_DEPTH)
		.map(|(_, root)| *root)
		.unwrap_or_else(empty_registry_root)
}

pub fn registry_proof_from_entries(
	kind: RecordKind,
	subject_commitment: H256,
	entry: Option<RegistryEntry>,
	entries: &[RegistryEntry],
) -> RegistryProof {
	let nodes = registry_nodes_from_entries(entries);
	let root = registry_root_from_entries(entries);
	build_registry_proof(kind, subject_commitment, entry, root, |level, path| {
		nodes.get(&(level, path)).copied()
	})
}

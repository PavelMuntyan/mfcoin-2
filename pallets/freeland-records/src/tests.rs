use crate::{
	mock::*, empty_registry_root, records_root, registry_index, verify_registry_proof, BlockRecordIds,
	Error, Event, FreelandBlockInfo, issuer_change_id, IssuerPermissions, LatestBlockInfo, LatestRecords, NextNonce,
	RecordAction, RecordKind, RegistryEntry, RegistryLeaves, RegistryRoot, TotalRecords,
	FREELAND_INFO_ENGINE_ID, REGISTRY_DEPTH,
};
use codec::Encode;
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use sp_core::H256;
use sp_runtime::{generic::DigestItem, DispatchError};

fn hash(value: u64) -> H256 {
	H256::from_low_u64_be(value)
}

fn change_allowed(kind: RecordKind, issuer: u64, allowed: bool) {
	System::set_block_number(1);
	let proposal_id = issuer_change_id(kind, issuer, allowed, 3);
	assert_ok!(FreelandRecords::set_issuer(RuntimeOrigin::signed(1), kind, issuer, allowed));
	System::set_block_number(3);
	assert_ok!(FreelandRecords::execute_issuer_change(RuntimeOrigin::signed(99), proposal_id));
}

fn allow(kind: RecordKind, issuer: u64) {
	change_allowed(kind, issuer, true);
}

fn submit_subject(
	issuer: u64,
	kind: RecordKind,
	action: RecordAction,
	subject_commitment: H256,
	nonce: u64,
) -> Result<(), DispatchError> {
	FreelandRecords::submit_record(
		RuntimeOrigin::signed(issuer),
		kind,
		action,
		subject_commitment,
		hash(20),
		nonce,
		hash(30),
	)
}

fn submit(
	issuer: u64,
	kind: RecordKind,
	action: RecordAction,
	nonce: u64,
) -> Result<(), DispatchError> {
	submit_subject(issuer, kind, action, hash(10), nonce)
}

#[test]
fn authorized_issuer_submits_typed_record() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		allow(RecordKind::Passport, 1);
		assert_ok!(submit(1, RecordKind::Passport, RecordAction::Issue, 0));

		let record = LatestRecords::<Test>::get(RecordKind::Passport, hash(10)).unwrap();
		assert_eq!(record.version, 1);
		assert_eq!(record.payload_hash, hash(20));
		assert_eq!(record.evidence_hash, hash(30));
		assert_eq!(record.issuer, 1);
		assert_eq!(NextNonce::<Test>::get(1), 1);
		assert_eq!(TotalRecords::<Test>::get(), 1);
		assert_eq!(BlockRecordIds::<Test>::get().len(), 1);
		assert_eq!(RegistryLeaves::<Test>::iter().count(), 1);
		let proof = FreelandRecords::registry_proof(RecordKind::Passport, hash(10));
		assert!(verify_registry_proof(&proof));
		assert!(System::events().iter().any(|event| matches!(
			event.event,
			RuntimeEvent::FreelandRecords(Event::RecordSubmitted { nonce: 0, .. })
		)));
	});
}

#[test]
fn only_original_subject_issuer_can_update_or_revoke_a_record() {
	new_test_ext().execute_with(|| {
		allow(RecordKind::Passport, 1);
		allow(RecordKind::Passport, 2);
		assert_ok!(submit(1, RecordKind::Passport, RecordAction::Issue, 0));
		assert_noop!(
			submit(2, RecordKind::Passport, RecordAction::Update, 0),
			Error::<Test>::SubjectIssuerMismatch
		);
		assert_ok!(submit(1, RecordKind::Passport, RecordAction::Update, 1));
		assert_noop!(
			submit(2, RecordKind::Passport, RecordAction::Revoke, 0),
			Error::<Test>::SubjectIssuerMismatch
		);
		assert_ok!(submit(1, RecordKind::Passport, RecordAction::Revoke, 2));
		assert_eq!(LatestRecords::<Test>::get(RecordKind::Passport, hash(10)).unwrap().issuer, 1);
		assert_eq!(TotalRecords::<Test>::get(), 3);
	});
}

#[test]
fn unauthorized_issuer_and_signed_role_management_are_rejected() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			submit(1, RecordKind::Passport, RecordAction::Issue, 0),
			Error::<Test>::UnauthorizedIssuer
		);
		assert_noop!(
			FreelandRecords::set_issuer(RuntimeOrigin::signed(2), RecordKind::Passport, 1, true),
			Error::<Test>::UnauthorizedGovernance
		);
		assert_eq!(TotalRecords::<Test>::get(), 0);
	});
}

#[test]
fn issuer_change_requires_governance_and_timelock() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		assert_noop!(
			FreelandRecords::set_issuer(RuntimeOrigin::signed(2), RecordKind::Passport, 7, true),
			Error::<Test>::UnauthorizedGovernance
		);
		assert_ok!(FreelandRecords::set_issuer(RuntimeOrigin::signed(1), RecordKind::Passport, 7, true));
		let (_, change) = crate::PendingIssuerChanges::<Test>::iter().next().unwrap();
		let proposal_id = issuer_change_id(change.kind, change.issuer, change.allowed, change.execute_at);
		assert!(crate::PendingIssuerChanges::<Test>::contains_key(proposal_id));
		assert_noop!(
			FreelandRecords::execute_issuer_change(RuntimeOrigin::signed(99), proposal_id),
			Error::<Test>::ProposalNotReady
		);
		assert_noop!(
			FreelandRecords::set_issuer(RuntimeOrigin::signed(1), RecordKind::Passport, 7, true),
			Error::<Test>::ProposalAlreadyExists
		);
		System::set_block_number(3);
		assert_ok!(FreelandRecords::execute_issuer_change(RuntimeOrigin::signed(99), proposal_id));
		assert!(crate::PendingIssuerChanges::<Test>::get(proposal_id).is_none());
		assert!(IssuerPermissions::<Test>::get(RecordKind::Passport, 7));
	});
}

#[test]
fn replay_is_rejected_and_next_nonce_is_accepted() {
	new_test_ext().execute_with(|| {
		allow(RecordKind::Document, 1);
		assert_ok!(submit(1, RecordKind::Document, RecordAction::Issue, 0));
		assert_noop!(
			submit(1, RecordKind::Document, RecordAction::Update, 0),
			Error::<Test>::InvalidNonce
		);
		assert_ok!(submit(1, RecordKind::Document, RecordAction::Update, 1));
		assert_eq!(TotalRecords::<Test>::get(), 2);
		assert_eq!(RegistryRoot::<Test>::get(), empty_registry_root());
	});
}

#[test]
fn invalid_action_and_revoked_issuer_are_rejected() {
	new_test_ext().execute_with(|| {
		allow(RecordKind::Passport, 1);
		assert_noop!(
			submit(1, RecordKind::Passport, RecordAction::Open, 0),
			Error::<Test>::InvalidAction
		);
		change_allowed(RecordKind::Passport, 1, false);
		assert!(!IssuerPermissions::<Test>::get(RecordKind::Passport, 1));
		assert_noop!(
			submit(1, RecordKind::Passport, RecordAction::Issue, 0),
			Error::<Test>::UnauthorizedIssuer
		);
	});
}

#[test]
fn block_record_limit_is_enforced() {
	new_test_ext().execute_with(|| {
		allow(RecordKind::Document, 1);
		for nonce in 0..64 {
			let action = if nonce == 0 { RecordAction::Issue } else { RecordAction::Update };
			assert_ok!(submit(1, RecordKind::Document, action, nonce));
		}
		assert_noop!(
			submit(1, RecordKind::Document, RecordAction::Update, 64),
			Error::<Test>::TooManyRecords
		);
		assert_eq!(NextNonce::<Test>::get(1), 64);
	});
}

#[test]
fn registry_supports_more_than_the_old_limit() {
	new_test_ext().execute_with(|| {
		allow(RecordKind::Passport, 1);
		for nonce in 0..256 {
			assert_ok!(submit_subject(
				1,
				RecordKind::Passport,
				RecordAction::Issue,
				hash(nonce + 1),
				nonce
			));
			if (nonce + 1) % 64 == 0 {
				FreelandRecords::on_finalize(nonce + 1);
			}
		}
		assert_eq!(NextNonce::<Test>::get(1), 256);
		assert_eq!(TotalRecords::<Test>::get(), 256);
		assert_eq!(RegistryLeaves::<Test>::iter().count(), 256);
		let proof = FreelandRecords::registry_proof(RecordKind::Passport, hash(256));
		assert!(verify_registry_proof(&proof));
	});
}

#[test]
fn registry_tracks_latest_state_and_is_order_independent() {
	let forward = |reverse: bool| {
		new_test_ext().execute_with(|| {
			allow(RecordKind::Passport, 1);
			allow(RecordKind::Citizen, 2);
			if reverse {
				assert_ok!(submit_subject(2, RecordKind::Citizen, RecordAction::Issue, hash(2), 0));
				assert_ok!(submit_subject(1, RecordKind::Passport, RecordAction::Issue, hash(1), 0));
			} else {
				assert_ok!(submit_subject(1, RecordKind::Passport, RecordAction::Issue, hash(1), 0));
				assert_ok!(submit_subject(2, RecordKind::Citizen, RecordAction::Issue, hash(2), 0));
			}
			assert_ne!(registry_index(RecordKind::Passport, hash(1)), registry_index(RecordKind::Citizen, hash(2)));
			assert_eq!(RegistryLeaves::<Test>::iter().count(), 2);
			let entries = RegistryLeaves::<Test>::iter().map(|(_, entry)| entry).collect::<Vec<_>>();
			assert_eq!(RegistryRoot::<Test>::get(), crate::registry_root_from_entries(&entries));
			assert!(verify_registry_proof(&FreelandRecords::registry_proof(RecordKind::Passport, hash(1))));
			assert!(verify_registry_proof(&FreelandRecords::registry_proof(RecordKind::Citizen, hash(2))));
			RegistryRoot::<Test>::get()
		})
	};
	assert_eq!(forward(false), forward(true));

	new_test_ext().execute_with(|| {
		allow(RecordKind::Passport, 1);
		assert_ok!(submit_subject(1, RecordKind::Passport, RecordAction::Issue, hash(1), 0));
		let issued = RegistryRoot::<Test>::get();
		assert_ok!(submit_subject(1, RecordKind::Passport, RecordAction::Revoke, hash(1), 1));
		let revoked = RegistryRoot::<Test>::get();
		assert_ne!(issued, revoked);
		assert_eq!(RegistryLeaves::<Test>::iter().count(), 1);
		let proof = FreelandRecords::registry_proof(RecordKind::Passport, hash(1));
		assert_eq!(proof.entry.unwrap().record_id, LatestRecords::<Test>::get(RecordKind::Passport, hash(1)).unwrap().id());
		assert!(verify_registry_proof(&proof));
	});
}

#[test]
fn proofs_detect_tampering_and_support_absence() {
	new_test_ext().execute_with(|| {
		allow(RecordKind::Passport, 1);
		assert_ok!(submit_subject(1, RecordKind::Passport, RecordAction::Issue, hash(1), 0));
		let proof = FreelandRecords::registry_proof(RecordKind::Passport, hash(1));
		assert!(verify_registry_proof(&proof));
		assert_eq!(proof.siblings.len(), REGISTRY_DEPTH as usize);

		let mut wrong_sibling = proof.clone();
		wrong_sibling.siblings[0].0[0] ^= 1;
		assert!(!verify_registry_proof(&wrong_sibling));
		let mut wrong_entry = proof.clone();
		wrong_entry.entry.as_mut().unwrap().record_id = hash(999);
		assert!(!verify_registry_proof(&wrong_entry));
		let mut wrong_length = proof.clone();
		wrong_length.siblings.pop();
		assert!(!verify_registry_proof(&wrong_length));

		let absence = FreelandRecords::registry_proof(RecordKind::Passport, hash(999));
		assert!(absence.entry.is_none());
		assert!(verify_registry_proof(&absence));
	});
}

#[test]
fn registry_index_collision_is_rejected() {
	new_test_ext().execute_with(|| {
		let index = registry_index(RecordKind::Passport, hash(1));
		RegistryLeaves::<Test>::set(index, Some(RegistryEntry {
			kind: RecordKind::Citizen,
			subject_commitment: hash(1),
			record_id: hash(2),
		}));
		assert!(matches!(
			FreelandRecords::update_registry(RegistryEntry {
				kind: RecordKind::Passport,
				subject_commitment: hash(1),
				record_id: hash(3),
			}),
			Err(Error::<Test>::RegistryIndexCollision)
		));
	});
}

#[test]
fn block_info_links_roots_and_deposits_digest() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		FreelandRecords::on_finalize(1);
		let first = LatestBlockInfo::<Test>::get().unwrap();
		assert_eq!(first.records_root, records_root(&[]));
		assert_eq!(first.registry_root, empty_registry_root());
		assert_eq!(first.records_count, 0);
		assert_eq!(first.previous_info_root, H256::zero());

		System::set_block_number(2);
		allow(RecordKind::Citizen, 1);
		assert_ok!(submit(1, RecordKind::Citizen, RecordAction::Issue, 0));
		let record_ids = BlockRecordIds::<Test>::get();
		FreelandRecords::on_finalize(2);
		let second = LatestBlockInfo::<Test>::get().unwrap();
		assert_eq!(second.previous_info_root, first.root());
		assert_eq!(second.records_root, records_root(&record_ids));
		assert_eq!(second.registry_root, RegistryRoot::<Test>::get());
		assert_eq!(second.records_count, 1);
		assert!(BlockRecordIds::<Test>::get().is_empty());
		assert!(System::digest().logs.iter().any(|log| {
			*log == DigestItem::Other((FREELAND_INFO_ENGINE_ID, second.root()).encode())
		}));
	});
}

#[test]
fn merkle_root_is_ordered_and_domain_separated() {
	let one = records_root(&[hash(1)]);
	assert_ne!(one, records_root(&[]));
	assert_ne!(one, records_root(&[hash(2)]));
	assert_ne!(records_root(&[hash(1), hash(2)]), records_root(&[hash(2), hash(1)]));
	assert_eq!(one, records_root(&[hash(1)]));
}

#[test]
fn block_info_type_has_explicit_reserved_fields() {
	let info = FreelandBlockInfo {
		version: 1,
		previous_info_root: H256::zero(),
		records_root: records_root(&[]),
		registry_root: empty_registry_root(),
		governance_root: H256::zero(),
		records_count: 0,
		epoch: 0,
		validator_set_hash: H256::zero(),
	};
	assert_eq!(info.root(), info.root());
}

#[test]
#[ignore]
fn synthetic_33k_registry_and_proofs() {
	new_test_ext().execute_with(|| {
		let started = std::time::Instant::now();
		for value in 1..=33_000u64 {
			let kind = if value % 2 == 0 { RecordKind::Citizen } else { RecordKind::Passport };
			FreelandRecords::update_registry(RegistryEntry {
				kind,
				subject_commitment: hash(value),
				record_id: hash(value + 100_000),
			})
			.unwrap();
		}
		assert_eq!(RegistryLeaves::<Test>::iter().count(), 33_000);
		assert_ne!(RegistryRoot::<Test>::get(), empty_registry_root());
		for value in [1, 16_500, 33_000] {
			let kind = if value % 2 == 0 { RecordKind::Citizen } else { RecordKind::Passport };
			let proof = FreelandRecords::registry_proof(kind, hash(value));
			assert!(verify_registry_proof(&proof));
		}
		println!("33K registry populated and verified in {:?}", started.elapsed());
	});
}

use crate::{
	mock::*, records_root, registry_root, BlockRecordIds, Error, Event, FreelandBlockInfo,
	IssuerPermissions, LatestBlockInfo, LatestRecords, NextNonce, RecordAction, RecordKind,
	RegistryEntries, TotalRecords, FREELAND_INFO_ENGINE_ID,
};
use codec::Encode;
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use sp_core::H256;
use sp_runtime::{generic::DigestItem, DispatchError};

fn hash(value: u64) -> H256 {
	H256::from_low_u64_be(value)
}

fn allow(kind: RecordKind, issuer: u64) {
	assert_ok!(FreelandRecords::set_issuer(RuntimeOrigin::root(), kind, issuer, true));
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
		assert_eq!(RegistryEntries::<Test>::get().len(), 1);
		assert!(System::events().iter().any(|event| matches!(
			event.event,
			RuntimeEvent::FreelandRecords(Event::RecordSubmitted { nonce: 0, .. })
		)));
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
			FreelandRecords::set_issuer(RuntimeOrigin::signed(1), RecordKind::Passport, 1, true),
			DispatchError::BadOrigin
		);
		assert_eq!(TotalRecords::<Test>::get(), 0);
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
		assert!(RegistryEntries::<Test>::get().is_empty());
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
		assert_ok!(FreelandRecords::set_issuer(
			RuntimeOrigin::root(),
			RecordKind::Passport,
			1,
			false
		));
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
fn registry_limit_is_enforced_without_consuming_nonce() {
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
		assert_noop!(
			submit_subject(1, RecordKind::Passport, RecordAction::Issue, hash(257), 256),
			Error::<Test>::RegistryFull
		);
		assert_eq!(NextNonce::<Test>::get(1), 256);
		assert_eq!(TotalRecords::<Test>::get(), 256);
		assert_eq!(RegistryEntries::<Test>::get().len(), 256);
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
			registry_root(&RegistryEntries::<Test>::get())
		})
	};
	assert_eq!(forward(false), forward(true));

	new_test_ext().execute_with(|| {
		allow(RecordKind::Passport, 1);
		assert_ok!(submit_subject(1, RecordKind::Passport, RecordAction::Issue, hash(1), 0));
		let issued = registry_root(&RegistryEntries::<Test>::get());
		assert_ok!(submit_subject(1, RecordKind::Passport, RecordAction::Revoke, hash(1), 1));
		let revoked = registry_root(&RegistryEntries::<Test>::get());
		assert_ne!(issued, revoked);
		assert_eq!(RegistryEntries::<Test>::get().len(), 1);
	});
}

#[test]
fn block_info_links_roots_and_deposits_digest() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		FreelandRecords::on_finalize(1);
		let first = LatestBlockInfo::<Test>::get().unwrap();
		assert_eq!(first.records_root, records_root(&[]));
		assert_eq!(first.registry_root, registry_root(&[]));
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
		assert_eq!(second.registry_root, registry_root(&RegistryEntries::<Test>::get()));
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
		registry_root: registry_root(&[]),
		governance_root: H256::zero(),
		records_count: 0,
		epoch: 0,
		validator_set_hash: H256::zero(),
	};
	assert_eq!(info.root(), info.root());
}

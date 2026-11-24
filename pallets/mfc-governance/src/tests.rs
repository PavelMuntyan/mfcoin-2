use crate::{mock::{new_test_ext, MfcGovernance, RuntimeOrigin, System, Test}, Error, GovernanceAuthorities, Proposal, Proposals, RuntimeCode};
use frame_support::{assert_noop, assert_ok};
use sp_core::crypto::AccountId32;
use sp_keyring::Ed25519Keyring;
use codec::Encode;

struct TestRuntimeVersion;

impl sp_core::traits::ReadRuntimeVersion for TestRuntimeVersion {
    fn read_runtime_version(&self, _wasm_code: &[u8], _ext: &mut dyn sp_externalities::Externalities) -> Result<Vec<u8>, String> {
        Ok(sp_version::RuntimeVersion { spec_version: 1, ..Default::default() }.encode())
    }
}

fn governance() -> sp_core::crypto::AccountId32 {
    AccountId32::new([1; 32])
}

fn code(value: Vec<u8>) -> RuntimeCode {
    RuntimeCode::try_from(value).unwrap()
}

fn authorities() -> GovernanceAuthorities {
    GovernanceAuthorities::try_from(vec![(Ed25519Keyring::Charlie.public().into(), 1)]).unwrap()
}

#[test]
fn proposals_require_governance_signature() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            MfcGovernance::rotate_grandpa(
                RuntimeOrigin::signed(AccountId32::new([2; 32])),
                authorities(),
                10,
                None,
                10,
            ),
            Error::<Test>::Unauthorized
        );
    });
}

#[test]
fn grandpa_rotation_executes_only_after_timelock() {
    new_test_ext().execute_with(|| {
        let next = authorities();
        let proposal: Proposal<AccountId32, u64> = Proposal::RotateGrandpa { next_authorities: next.clone(), in_blocks: 5, forced: None, execute_at: 10 };
        let proposal_id = proposal.id();
        assert_ok!(MfcGovernance::rotate_grandpa(RuntimeOrigin::signed(governance()), next.clone(), 5, None, 10));
        assert!(pallet_grandpa::PendingChange::<Test>::get().is_none());
        assert_noop!(MfcGovernance::execute(RuntimeOrigin::signed(governance()), proposal_id), Error::<Test>::DelayTooShort);

        System::set_block_number(10);
        assert_ok!(MfcGovernance::execute(RuntimeOrigin::signed(governance()), proposal_id));
        let change = pallet_grandpa::PendingChange::<Test>::get().expect("rotation must be scheduled");
        assert_eq!(change.delay, 5);
        assert_eq!(change.next_authorities.into_inner(), next.into_inner());
        assert!(Proposals::<Test>::get(proposal_id).is_none());
    });
}

#[test]
fn duplicate_proposal_is_rejected() {
    new_test_ext().execute_with(|| {
        assert_ok!(MfcGovernance::rotate_grandpa(RuntimeOrigin::signed(governance()), authorities(), 5, None, 10));
        assert_noop!(
            MfcGovernance::rotate_grandpa(RuntimeOrigin::signed(governance()), authorities(), 5, None, 10),
            Error::<Test>::ProposalAlreadyExists
        );
    });
}

#[test]
fn runtime_code_executes_from_a_scheduled_proposal() {
    let mut ext = new_test_ext();
    ext.register_extension(sp_core::traits::ReadRuntimeVersionExt::new(TestRuntimeVersion));
    ext.execute_with(|| {
        let proposal: Proposal<AccountId32, u64> = Proposal::SetCode { code: code(vec![1, 2, 3]), execute_at: 10 };
        let proposal_id = proposal.id();
        assert_ok!(MfcGovernance::set_code(RuntimeOrigin::signed(governance()), code(vec![1, 2, 3]), 10));
        System::set_block_number(10);
        assert_ok!(MfcGovernance::execute(RuntimeOrigin::signed(governance()), proposal_id));
        assert_eq!(sp_io::storage::get(sp_storage::well_known_keys::CODE).map(|code| code.to_vec()), Some(vec![1, 2, 3]));
    });
}

#[test]
fn governance_account_can_rotate_after_timelock() {
    new_test_ext().execute_with(|| {
        let new = AccountId32::new([9; 32]);
        let proposal: Proposal<AccountId32, u64> = Proposal::SetGovernanceAccount { new: new.clone(), execute_at: 10 };
        let proposal_id = proposal.id();
        assert_ok!(MfcGovernance::set_governance_account(RuntimeOrigin::signed(governance()), new.clone(), 10));
        System::set_block_number(10);
        assert_ok!(MfcGovernance::execute(RuntimeOrigin::signed(governance()), proposal_id));
        assert_noop!(
            MfcGovernance::set_code(RuntimeOrigin::signed(governance()), code(vec![1]), 20),
            Error::<Test>::Unauthorized
        );
        assert_ok!(MfcGovernance::set_code(RuntimeOrigin::signed(new), code(vec![1]), 20));
    });
}

#[test]
fn governance_can_cancel_a_proposal() {
    new_test_ext().execute_with(|| {
        let proposal: Proposal<AccountId32, u64> = Proposal::SetCode { code: code(vec![1]), execute_at: 10 };
        let proposal_id = proposal.id();
        assert_ok!(MfcGovernance::set_code(RuntimeOrigin::signed(governance()), code(vec![1]), 10));
        assert_noop!(
            MfcGovernance::cancel(RuntimeOrigin::signed(AccountId32::new([2; 32])), proposal_id),
            Error::<Test>::Unauthorized
        );
        assert_ok!(MfcGovernance::cancel(RuntimeOrigin::signed(governance()), proposal_id));
        assert_noop!(
            MfcGovernance::execute(RuntimeOrigin::signed(governance()), proposal_id),
            Error::<Test>::ProposalNotFound
        );
    });
}

#[test]
fn expired_proposal_cannot_execute() {
    new_test_ext().execute_with(|| {
        let proposal: Proposal<AccountId32, u64> = Proposal::RotateGrandpa { next_authorities: authorities(), in_blocks: 5, forced: None, execute_at: 10 };
        let proposal_id = proposal.id();
        assert_ok!(MfcGovernance::rotate_grandpa(RuntimeOrigin::signed(governance()), authorities(), 5, None, 10));
        System::set_block_number(31);
        assert_noop!(
            MfcGovernance::execute(RuntimeOrigin::signed(governance()), proposal_id),
            Error::<Test>::ProposalExpired
        );
    });
}

#[test]
fn proposals_from_previous_governance_epoch_are_stale() {
    new_test_ext().execute_with(|| {
        let new = AccountId32::new([9; 32]);
        let rotation: Proposal<AccountId32, u64> = Proposal::SetGovernanceAccount { new: new.clone(), execute_at: 10 };
        let rotation_id = rotation.id();
        let stale: Proposal<AccountId32, u64> = Proposal::RotateGrandpa { next_authorities: authorities(), in_blocks: 5, forced: None, execute_at: 20 };
        let stale_id = stale.id();
        assert_ok!(MfcGovernance::set_governance_account(RuntimeOrigin::signed(governance()), new, 10));
        assert_ok!(MfcGovernance::rotate_grandpa(RuntimeOrigin::signed(governance()), authorities(), 5, None, 20));
        System::set_block_number(10);
        assert_ok!(MfcGovernance::execute(RuntimeOrigin::signed(governance()), rotation_id));
        System::set_block_number(20);
        assert_noop!(
            MfcGovernance::execute(RuntimeOrigin::signed(governance()), stale_id),
            Error::<Test>::StaleProposal
        );
    });
}

#[test]
fn empty_or_oversized_inputs_are_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            MfcGovernance::rotate_grandpa(RuntimeOrigin::signed(governance()), GovernanceAuthorities::default(), 5, None, 10),
            Error::<Test>::EmptyAuthorities
        );
        assert!(GovernanceAuthorities::try_from(vec![(Ed25519Keyring::Alice.public().into(), 1); 33]).is_err());
        assert!(RuntimeCode::try_from(vec![0; 3_000_001]).is_err());
    });
}

#[test]
fn execution_weight_accounts_for_runtime_code_size() {
    new_test_ext().execute_with(|| {
        let small: Proposal<AccountId32, u64> = Proposal::SetCode { code: code(vec![1]), execute_at: 10 };
        let large: Proposal<AccountId32, u64> = Proposal::SetCode { code: code(vec![0; 100_000]), execute_at: 10 };
        let small_id = small.id();
        let large_id = large.id();
        Proposals::<Test>::insert(small_id, small);
        Proposals::<Test>::insert(large_id, large);
        let small_weight = MfcGovernance::execute_weight(small_id);
        let large_weight = MfcGovernance::execute_weight(large_id);
        assert!(small_weight.ref_time() < large_weight.ref_time());
        assert!(small_weight.proof_size() < large_weight.proof_size());
    });
}

#[test]
fn proposals_cannot_be_scheduled_beyond_the_allowed_horizon() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            MfcGovernance::set_code(RuntimeOrigin::signed(governance()), code(vec![1]), 41),
            Error::<Test>::ScheduleTooFar
        );
    });
}

#[test]
fn recovery_has_a_shorter_delay_than_runtime_upgrades() {
    new_test_ext().execute_with(|| {
        let proposal: Proposal<AccountId32, u64> = Proposal::NoteStalled { delay: 1, best_finalized_block_number: 1, execute_at: 2 };
        let proposal_id = proposal.id();
        assert_ok!(MfcGovernance::note_stalled(RuntimeOrigin::signed(governance()), 1, 1, 2));
        System::set_block_number(2);
        assert_ok!(MfcGovernance::execute(RuntimeOrigin::signed(governance()), proposal_id));
    });
}

use crate::{mock::{new_test_ext, MfcGovernance, RuntimeOrigin, System, Test}, Error, Proposal, Proposals};
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

#[test]
fn proposals_require_governance_signature() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            MfcGovernance::rotate_grandpa(
                RuntimeOrigin::signed(AccountId32::new([2; 32])),
                vec![(Ed25519Keyring::Charlie.public().into(), 1)],
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
        let next = vec![(Ed25519Keyring::Charlie.public().into(), 1)];
        let proposal: Proposal<AccountId32, u64> = Proposal::RotateGrandpa { next_authorities: next.clone(), in_blocks: 5, forced: None, execute_at: 10 };
        let proposal_id = proposal.id();
        assert_ok!(MfcGovernance::rotate_grandpa(RuntimeOrigin::signed(AccountId32::new([1; 32])), next.clone(), 5, None, 10));
        assert!(pallet_grandpa::PendingChange::<Test>::get().is_none());
        assert_noop!(MfcGovernance::execute(RuntimeOrigin::signed(AccountId32::new([1; 32])), proposal_id), Error::<Test>::DelayTooShort);

        System::set_block_number(10);
        assert_ok!(MfcGovernance::execute(RuntimeOrigin::signed(AccountId32::new([1; 32])), proposal_id));
        let change = pallet_grandpa::PendingChange::<Test>::get().expect("rotation must be scheduled");
        assert_eq!(change.delay, 5);
        assert_eq!(change.next_authorities.into_inner(), next);
        assert!(Proposals::<Test>::get(proposal_id).is_none());
    });
}

#[test]
fn duplicate_proposal_is_rejected() {
    new_test_ext().execute_with(|| {
        let next = vec![(Ed25519Keyring::Charlie.public().into(), 1)];
        assert_ok!(MfcGovernance::rotate_grandpa(RuntimeOrigin::signed(AccountId32::new([1; 32])), next.clone(), 5, None, 10));
        assert_noop!(
            MfcGovernance::rotate_grandpa(RuntimeOrigin::signed(AccountId32::new([1; 32])), next, 5, None, 10),
            Error::<Test>::ProposalAlreadyExists
        );
    });
}

#[test]
fn runtime_code_executes_from_a_scheduled_proposal() {
    let mut ext = new_test_ext();
    ext.register_extension(sp_core::traits::ReadRuntimeVersionExt::new(TestRuntimeVersion));
    ext.execute_with(|| {
        let proposal: Proposal<AccountId32, u64> = Proposal::SetCode { code: vec![1, 2, 3], execute_at: 10 };
        let proposal_id = proposal.id();
        assert_ok!(MfcGovernance::set_code(RuntimeOrigin::signed(AccountId32::new([1; 32])), vec![1, 2, 3], 10));
        System::set_block_number(10);
        assert_ok!(MfcGovernance::execute(RuntimeOrigin::signed(AccountId32::new([1; 32])), proposal_id));
        assert_eq!(sp_io::storage::get(sp_storage::well_known_keys::CODE).map(|code| code.to_vec()), Some(vec![1, 2, 3]));
    });
}

#[test]
fn governance_account_can_rotate_after_timelock() {
    new_test_ext().execute_with(|| {
        let new = AccountId32::new([9; 32]);
        let proposal: Proposal<AccountId32, u64> = Proposal::SetGovernanceAccount { new: new.clone(), execute_at: 10 };
        let proposal_id = proposal.id();
        assert_ok!(MfcGovernance::set_governance_account(RuntimeOrigin::signed(AccountId32::new([1; 32])), new.clone(), 10));
        System::set_block_number(10);
        assert_ok!(MfcGovernance::execute(RuntimeOrigin::signed(AccountId32::new([1; 32])), proposal_id));
        assert_noop!(
            MfcGovernance::set_code(RuntimeOrigin::signed(AccountId32::new([1; 32])), vec![1], 20),
            Error::<Test>::Unauthorized
        );
        assert_ok!(MfcGovernance::set_code(RuntimeOrigin::signed(new), vec![1], 20));
    });
}

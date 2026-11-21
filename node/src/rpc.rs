//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::sync::Arc;

use jsonrpsee::{
	core::RpcResult,
	proc_macros::rpc,
	types::{error::{INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE}, ErrorObjectOwned},
	RpcModule,
};
use pallet_freeland_records::{
	FreelandApi as FreelandRuntimeApi, FreelandBlockInfo, FreelandRecord, IssuerChange, RecordAction,
	RecordKind, RegistryProof,
};
use sc_transaction_pool_api::TransactionPool;
use serde_json::{json, Value};
use solochain_template_runtime::{opaque::Block, AccountId, Balance, Nonce};
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_core::H256;

/// Full client dependencies.
pub struct FullDeps<C, P> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
}

/// Read-only Freeland registry queries.
pub struct FreelandRpc<C> {
	client: Arc<C>,
}

impl<C> FreelandRpc<C> {
	/// Create a Freeland RPC handler.
	pub fn new(client: Arc<C>) -> Self {
		Self { client }
	}
}

#[rpc(server, namespace = "freeland")]
pub trait Freeland {
	/// Return the latest Freeland block informer at an optional block hash.
	#[method(name = "latestBlockInfo")]
	fn latest_block_info(&self, at: Option<H256>) -> RpcResult<Option<Value>>;

	/// Return the latest record for a kind and subject commitment.
	#[method(name = "latestRecord")]
	fn latest_record(
		&self,
		kind: String,
		subject_commitment: H256,
		at: Option<H256>,
	) -> RpcResult<Option<Value>>;

	/// Return a sparse Merkle proof for a registry subject.
	#[method(name = "registryProof")]
	fn registry_proof(
		&self,
		kind: String,
		subject_commitment: H256,
		at: Option<H256>,
	) -> RpcResult<RegistryProof>;

	/// Return Freeland records submitted in a block with their extrinsic indexes.
	#[method(name = "blockRecords")]
	fn block_records(&self, at: Option<H256>) -> RpcResult<Vec<Value>>;

	/// Return a bounded page of records from a block.
	#[method(name = "blockRecordsPage")]
	fn block_records_page(&self, offset: u32, limit: u32, at: Option<H256>) -> RpcResult<Vec<Value>>;

	/// Return the total number of Freeland records in the current state.
	#[method(name = "totalRecords")]
	fn total_records(&self, at: Option<H256>) -> RpcResult<u64>;

	/// Return issuer changes waiting for their timelock to expire.
	#[method(name = "pendingIssuerChanges")]
	fn pending_issuer_changes(&self, at: Option<H256>) -> RpcResult<Vec<Value>>;
}

impl<C> FreelandServer for FreelandRpc<C>
where
	C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
	C::Api: FreelandRuntimeApi<Block, AccountId>,
{
	fn latest_block_info(&self, at: Option<H256>) -> RpcResult<Option<Value>> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);
		let info = self
			.client
			.runtime_api()
			.latest_block_info(at)
			.map_err(runtime_error)?;
		Ok(info.map(block_info_json))
	}

	fn latest_record(
		&self,
		kind: String,
		subject_commitment: H256,
		at: Option<H256>,
	) -> RpcResult<Option<Value>> {
		let kind = parse_record_kind(&kind)?;
		let at = at.unwrap_or_else(|| self.client.info().best_hash);
		let record = self
			.client
			.runtime_api()
			.latest_record(at, kind, subject_commitment)
			.map_err(runtime_error)?;
		Ok(record.map(record_json))
	}

	fn registry_proof(
		&self,
		kind: String,
		subject_commitment: H256,
		at: Option<H256>,
	) -> RpcResult<RegistryProof> {
		let kind = parse_record_kind(&kind)?;
		let at = at.unwrap_or_else(|| self.client.info().best_hash);
		self.client
			.runtime_api()
			.registry_proof(at, kind, subject_commitment)
			.map_err(runtime_error)
	}

	fn block_records(&self, at: Option<H256>) -> RpcResult<Vec<Value>> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);
		let records = self
			.client
			.runtime_api()
			.block_records(at)
			.map_err(runtime_error)?;
		Ok(records
			.into_iter()
			.map(|(extrinsic_index, record)| {
				let mut value = record_json(record);
				value["extrinsicIndex"] = extrinsic_index.into();
				value
			})
			.collect())
	}

	fn block_records_page(&self, offset: u32, limit: u32, at: Option<H256>) -> RpcResult<Vec<Value>> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);
		let records = self
			.client
			.runtime_api()
			.block_records_page(at, offset, limit)
			.map_err(runtime_error)?;
		Ok(records
			.into_iter()
			.map(|(extrinsic_index, record)| {
				let mut value = record_json(record);
				value["extrinsicIndex"] = extrinsic_index.into();
				value
			})
			.collect())
	}

	fn total_records(&self, at: Option<H256>) -> RpcResult<u64> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);
		self.client.runtime_api().total_records(at).map_err(runtime_error)
	}

	fn pending_issuer_changes(&self, at: Option<H256>) -> RpcResult<Vec<Value>> {
		let at = at.unwrap_or_else(|| self.client.info().best_hash);
		let changes = self
			.client
			.runtime_api()
			.pending_issuer_changes(at)
			.map_err(runtime_error)?;
		Ok(changes
			.into_iter()
			.map(|(proposal_id, change)| {
				let IssuerChange { kind, issuer, allowed, execute_at } = change;
				json!({
					"proposalId": proposal_id,
					"kind": kind_name(kind),
					"issuer": issuer,
					"allowed": allowed,
					"executeAt": execute_at,
				})
			})
			.collect())
	}
}

fn runtime_error(error: impl std::fmt::Display) -> ErrorObjectOwned {
	ErrorObjectOwned::owned(INTERNAL_ERROR_CODE, error.to_string(), None::<()>)
}

fn parse_record_kind(value: &str) -> RpcResult<RecordKind> {
	match value.to_ascii_lowercase().as_str() {
		"passport" => Ok(RecordKind::Passport),
		"citizen" => Ok(RecordKind::Citizen),
		"election" => Ok(RecordKind::Election),
		"document" => Ok(RecordKind::Document),
		"revocation" => Ok(RecordKind::Revocation),
		_ => Err(ErrorObjectOwned::owned(
			INVALID_PARAMS_CODE,
			format!("Unknown Freeland record kind: {value}"),
			None::<()>,
		)),
	}
}

fn kind_name(kind: RecordKind) -> &'static str {
	match kind {
		RecordKind::Passport => "passport",
		RecordKind::Citizen => "citizen",
		RecordKind::Election => "election",
		RecordKind::Document => "document",
		RecordKind::Revocation => "revocation",
	}
}

fn action_name(action: RecordAction) -> &'static str {
	match action {
		RecordAction::Issue => "issue",
		RecordAction::Renew => "renew",
		RecordAction::Revoke => "revoke",
		RecordAction::Update => "update",
		RecordAction::Open => "open",
		RecordAction::Commit => "commit",
		RecordAction::Reveal => "reveal",
		RecordAction::Close => "close",
	}
}

fn block_info_json(info: FreelandBlockInfo) -> Value {
	json!({
		"version": info.version,
		"previousInfoRoot": info.previous_info_root,
		"recordsRoot": info.records_root,
		"registryRoot": info.registry_root,
		"governanceRoot": info.governance_root,
		"recordsCount": info.records_count,
		"epoch": info.epoch,
		"validatorSetHash": info.validator_set_hash,
		"infoRoot": info.root(),
	})
}

fn record_json(record: FreelandRecord<AccountId>) -> Value {
	json!({
		"recordId": record.id(),
		"version": record.version,
		"kind": kind_name(record.kind),
		"action": action_name(record.action),
		"subjectCommitment": record.subject_commitment,
		"payloadHash": record.payload_hash,
		"issuer": record.issuer,
		"nonce": record.nonce,
		"evidenceHash": record.evidence_hash,
	})
}

/// Instantiate all full RPC extensions.
pub fn create_full<C, P>(
	deps: FullDeps<C, P>,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
	C: ProvideRuntimeApi<Block>,
	C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError> + 'static,
	C: Send + Sync + 'static,
	C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
	C::Api: BlockBuilder<Block>,
	C::Api: FreelandRuntimeApi<Block, AccountId>,
	P: TransactionPool + 'static,
{
	use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
	use substrate_frame_rpc_system::{System, SystemApiServer};

	let mut module = RpcModule::new(());
	let FullDeps { client, pool } = deps;

	module.merge(System::new(client.clone(), pool).into_rpc())?;
	module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
	module.merge(FreelandRpc::new(client).into_rpc())?;

	Ok(module)
}

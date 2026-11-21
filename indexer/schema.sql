CREATE TABLE IF NOT EXISTS indexer_state (
  id SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
  best_hash TEXT NOT NULL,
  best_number BIGINT NOT NULL,
  finalized_hash TEXT NOT NULL,
  finalized_number BIGINT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS freeland_block_info (
  block_hash TEXT PRIMARY KEY,
  block_number BIGINT NOT NULL,
  parent_hash TEXT NOT NULL,
  previous_info_root TEXT,
  records_root TEXT,
  registry_root TEXT,
  governance_root TEXT,
  records_count INTEGER,
  epoch BIGINT,
  validator_set_hash TEXT,
  info_root TEXT,
  status TEXT NOT NULL CHECK (status IN ('included', 'orphaned', 'finalized')),
  indexed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS freeland_block_info_canonical_number
  ON freeland_block_info(block_number) WHERE status != 'orphaned';

CREATE TABLE IF NOT EXISTS freeland_records (
  id BIGSERIAL PRIMARY KEY,
  record_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  action TEXT NOT NULL,
  subject_commitment TEXT NOT NULL,
  payload_hash TEXT NOT NULL,
  evidence_hash TEXT NOT NULL,
  issuer TEXT NOT NULL,
  nonce BIGINT NOT NULL,
  block_hash TEXT NOT NULL REFERENCES freeland_block_info(block_hash),
  block_number BIGINT NOT NULL,
  extrinsic_index INTEGER NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('included', 'orphaned', 'finalized')),
  indexed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (block_hash, extrinsic_index)
);

CREATE INDEX IF NOT EXISTS freeland_records_subject
  ON freeland_records(subject_commitment, block_number DESC);
CREATE INDEX IF NOT EXISTS freeland_records_status
  ON freeland_records(status, block_number);

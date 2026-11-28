use crate::service::FullClient;
use sc_consensus_pow::{Error, PowAlgorithm, PowAux};
use solochain_template_runtime::{opaque::Block, MILLI_SECS_PER_BLOCK};
use sp_api::ProvideRuntimeApi;
use sp_consensus_pow::TimestampApi;
use sp_core::U256;
use sp_runtime::{generic::BlockId, traits::Header as HeaderT};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;

type BlockHash = <Block as sp_runtime::traits::Block>::Hash;
type DifficultyReader = Arc<dyn Fn(BlockHash) -> Result<U256, Error<Block>> + Send + Sync>;
type TimestampReader = Arc<dyn Fn(BlockHash) -> Result<u64, Error<Block>> + Send + Sync>;

pub struct MiningLifecycle {
    stop: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
}

impl MiningLifecycle {
    pub fn new(stop: Arc<AtomicBool>, join_handle: JoinHandle<()>) -> Self {
        Self { stop, join_handle: Some(join_handle) }
    }
}

impl Drop for MiningLifecycle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Clone)]
pub struct Sha256Algorithm {
    target: U256,
    client: Option<Arc<FullClient>>,
    difficulty_reader: Option<DifficultyReader>,
    timestamp_reader: Option<TimestampReader>,
    window: u32,
}

impl Default for Sha256Algorithm {
    fn default() -> Self {
        Self {
            target: U256::MAX / 25_000,
            client: None,
            difficulty_reader: None,
            timestamp_reader: None,
            window: 60,
        }
    }
}

fn missing_aux_target(block_number: u32) -> Option<U256> {
    (block_number == 0).then(|| U256::MAX / 25_000)
}

impl Sha256Algorithm {
    pub fn new(client: Arc<FullClient>) -> Self {
        let mut algorithm = Self::default();
        algorithm.difficulty_reader = Some(Arc::new({
            let client = client.clone();
            move |parent| {
                let header = client
                    .header(parent)
                    .map_err(|error| Error::Other(error.to_string()))?
                    .ok_or(Error::NoBestHeader)?;
                let aux: PowAux<U256> = PowAux::read::<_, Block>(client.as_ref(), &parent)?;
                if aux.difficulty.is_zero() {
                    missing_aux_target(*header.number())
                        .ok_or_else(|| Error::Other("Missing PoW auxiliary difficulty after genesis".to_string()))
                } else {
                    Ok(work_to_target(aux.difficulty))
                }
            }
        }));
        algorithm.timestamp_reader = Some(Arc::new({
            let client = client.clone();
            move |hash| {
                client
                    .runtime_api()
                    .timestamp(hash)
                    .map_err(|error| Error::Other(error.to_string()))
            }
        }));
        algorithm.client = Some(client);
        algorithm
    }

    fn timestamp(&self, hash: BlockHash) -> Result<u64, Error<Block>> {
        let reader = self.timestamp_reader.as_ref().ok_or_else(|| {
            Error::Other("Timestamp reader is not configured".to_string())
        })?;
        reader(hash)
    }
}

fn target_to_work(target: U256) -> U256 {
    if target.is_zero() {
        return U256::MAX;
    }
    U256::MAX / target
}

fn work_to_target(work: U256) -> U256 {
    if work.is_zero() {
        return U256::zero();
    }
    U256::MAX / work
}

impl PowAlgorithm<Block> for Sha256Algorithm {
    type Difficulty = U256;

    fn difficulty(&self, parent: BlockHash) -> Result<Self::Difficulty, Error<Block>> {
        let current = match &self.difficulty_reader {
            Some(reader) => reader(parent)?,
            None => self.target,
        };
        let Some(client) = &self.client else {
            return Ok(target_to_work(current));
        };
        let mut ancestor_header = client
            .header(parent)
            .map_err(|error| Error::Other(error.to_string()))?
            .ok_or(Error::NoBestHeader)?;
        let number = *ancestor_header.number();
        if number < self.window {
            return Ok(target_to_work(current));
        }
        for _ in 0..self.window {
            let ancestor = *ancestor_header.parent_hash();
            ancestor_header = client
                .header(ancestor)
                .map_err(|error| Error::Other(error.to_string()))?
                .ok_or(Error::NoBestHeader)?;
        }
        let elapsed = self
            .timestamp(parent)?
            .saturating_sub(self.timestamp(ancestor_header.hash())?);
        Ok(target_to_work(retarget(current, elapsed, self.window)))
    }

    fn verify(
        &self,
        _parent: &BlockId<Block>,
        pre_hash: &BlockHash,
        _pre_digest: Option<&[u8]>,
        seal: &Vec<u8>,
        difficulty: Self::Difficulty,
    ) -> Result<bool, Error<Block>> {
        let mut payload = pre_hash.as_bytes().to_vec();
        payload.extend_from_slice(seal);
        let hash = sp_crypto_hashing::sha2_256(&payload);
        Ok(U256::from_big_endian(&hash) <= work_to_target(difficulty))
    }
}

fn retarget(current: U256, elapsed: u64, window: u32) -> U256 {
    if elapsed == 0 {
        return current;
    }
    let expected = u64::from(window) * MILLI_SECS_PER_BLOCK;
    let next = current.saturating_mul(U256::from(elapsed)) / U256::from(expected);
    let minimum = current.saturating_mul(U256::from(9)) / U256::from(10);
    let maximum = current.saturating_mul(U256::from(21)) / U256::from(20);
    next.max(minimum).min(maximum)
}

pub fn find_seal(pre_hash: &BlockHash, difficulty: U256) -> Vec<u8> {
    let target = work_to_target(difficulty);
    let mut nonce = 0u64;
    loop {
        let seal = nonce.to_le_bytes().to_vec();
        let mut payload = pre_hash.as_bytes().to_vec();
        payload.extend_from_slice(&seal);
        let hash = sp_crypto_hashing::sha2_256(&payload);
        if U256::from_big_endian(&hash) <= target {
            return seal;
        }
        nonce = nonce.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_seal_when_target_is_maximum() {
        let algorithm = Sha256Algorithm { target: U256::MAX, ..Default::default() };
        let parent = BlockId::Hash(Default::default());
        let pre_hash = Default::default();
        assert!(algorithm
            .verify(&parent, &pre_hash, None, &vec![0], target_to_work(U256::MAX))
            .unwrap());
    }

    #[test]
    fn rejects_seal_when_work_is_zero() {
        let algorithm = Sha256Algorithm::default();
        let parent = BlockId::Hash(Default::default());
        let pre_hash = Default::default();
        assert!(!algorithm
            .verify(&parent, &pre_hash, None, &vec![0], U256::from(0))
            .unwrap());
    }

    #[test]
    fn finds_valid_seal() {
        let pre_hash = Default::default();
        let difficulty = target_to_work(U256::MAX / 25_000);
        let seal = find_seal(&pre_hash, difficulty);
        let algorithm = Sha256Algorithm::default();
        let parent = BlockId::Hash(Default::default());
        assert!(algorithm
            .verify(&parent, &pre_hash, None, &seal, difficulty)
            .unwrap());
    }

    #[test]
    fn difficulty_returns_work_for_the_consensus_engine() {
        let algorithm = Sha256Algorithm::default();
        let work = algorithm.difficulty(Default::default()).unwrap();
        assert_eq!(work, target_to_work(U256::MAX / 25_000));
        assert!(work_to_target(work) > U256::zero());
    }

    #[test]
    fn missing_aux_is_allowed_only_at_genesis() {
        assert_eq!(missing_aux_target(0), Some(U256::MAX / 25_000));
        assert_eq!(missing_aux_target(1), None);
        assert_eq!(missing_aux_target(60), None);
    }

    #[test]
    fn harder_targets_contribute_more_work_to_fork_choice() {
        let easy_target = U256::from(1_000);
        let hard_target = U256::from(500);
        assert!(target_to_work(hard_target) > target_to_work(easy_target));

        let easy_chain_work = target_to_work(easy_target).saturating_mul(U256::from(2));
        let hard_chain_work = target_to_work(hard_target).saturating_mul(U256::from(2));
        assert!(hard_chain_work > easy_chain_work);
    }

    #[test]
    fn retargets_towards_the_runtime_block_interval() {
        assert_eq!(retarget(U256::from(1_000), 150_000, 60), U256::from(900));
        assert_eq!(retarget(U256::from(1_000), 225_000, 60), U256::from(900));
        assert_eq!(retarget(U256::from(1_000), 90_000, 60), U256::from(900));
        assert_eq!(retarget(U256::from(1_000), 360_000, 60), U256::from(1_000));
        assert_eq!(retarget(U256::from(1_000), 480_000, 60), U256::from(1_050));
        assert_eq!(retarget(U256::from(1_000), 1, 60), U256::from(900));
        assert_eq!(retarget(U256::from(1_000), 10_000, 60), U256::from(900));
        assert_eq!(retarget(U256::from(1_000), 0, 60), U256::from(1_000));

        assert!(retarget(U256::from(1_000), 480_000, 60) > U256::from(1_000));
        assert!(retarget(U256::from(1_000), 240_000, 60) < U256::from(1_000));
    }

    #[test]
    fn retarget_limits_each_block_change_and_recovers_after_a_hashrate_shock() {
        let expected = 60u64 * MILLI_SECS_PER_BLOCK;
        assert_eq!(retarget(U256::from(1_000), expected / 100, 60), U256::from(900));
        assert_eq!(retarget(U256::from(1_000), expected * 100, 60), U256::from(1_050));

        let mut target = U256::from(50);
        for _ in 0..60 {
            target = retarget(target, expected * 100, 60);
        }
        assert!(target > U256::from(50));
    }

    #[test]
    fn retarget_converges_with_constant_hashrate() {
        let window = 60;
        let expected = u64::from(window) * MILLI_SECS_PER_BLOCK;
        let target_block_interval = 1_000_000u64;
        let mut target = U256::from(1_000);

        for _ in 0..500 {
            let elapsed = target_block_interval.saturating_mul(expected) / target.as_u64();
            target = retarget(target, elapsed, window);
        }

        assert!(target > U256::from(990_000));
        assert!(target < U256::from(1_010_000));
    }
}

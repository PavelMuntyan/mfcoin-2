use sc_consensus_pow::{Error, PowAlgorithm};
use solochain_template_runtime::opaque::Block;
use sp_core::U256;
use sp_runtime::generic::BlockId;

type BlockHash = <Block as sp_runtime::traits::Block>::Hash;
#[derive(Clone)]
pub struct Sha256Algorithm {
    target: U256,
}

impl Default for Sha256Algorithm {
    fn default() -> Self {
        Self {
            target: U256::MAX / 25_000,
        }
    }
}

impl PowAlgorithm<Block> for Sha256Algorithm {
    type Difficulty = U256;

    fn difficulty(&self, _parent: BlockHash) -> Result<Self::Difficulty, Error<Block>> {
        Ok(self.target)
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
        Ok(U256::from_big_endian(&hash) <= difficulty)
    }
}

pub fn find_seal(pre_hash: &BlockHash, difficulty: U256) -> Vec<u8> {
    let mut nonce = 0u64;
    loop {
        let seal = nonce.to_le_bytes().to_vec();
        let mut payload = pre_hash.as_bytes().to_vec();
        payload.extend_from_slice(&seal);
        let hash = sp_crypto_hashing::sha2_256(&payload);
        if U256::from_big_endian(&hash) <= difficulty {
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
        let algorithm = Sha256Algorithm { target: U256::MAX };
        let parent = BlockId::Hash(Default::default());
        let pre_hash = Default::default();
        assert!(algorithm
            .verify(&parent, &pre_hash, None, &vec![0], U256::MAX)
            .unwrap());
    }

    #[test]
    fn rejects_seal_when_target_is_zero() {
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
        let difficulty = U256::MAX / 25_000;
        let seal = find_seal(&pre_hash, difficulty);
        let algorithm = Sha256Algorithm::default();
        let parent = BlockId::Hash(Default::default());
        assert!(algorithm
            .verify(&parent, &pre_hash, None, &seal, difficulty)
            .unwrap());
    }
}

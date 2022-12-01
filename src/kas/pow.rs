use kaspow::{PowHash, Uint256};

use crate::kas::proto::{Job, Solution};
use crate::util::atomic_id;

const POWHASH_SIZE: usize = 25;

#[derive(Clone)]
pub struct Computer {
    hasher: PowHash,
    // testnet: bool
}

impl Computer {
    pub fn new(_testnet: bool) -> Self {
        let this = Self { hasher: Default::default() };
        assert_eq!(this.hasher.0.len(), POWHASH_SIZE);
        this
    }
    #[inline]
    pub fn compute_raw(&mut self, job: &Job, nonce: u64) -> Solution {
        // last finalize_with_nonce have polluted it
        unsafe {
            std::ptr::copy_nonoverlapping((&job.matrixhasher.1 .0[..]).as_ptr(), (&mut self.hasher.0[..]).as_mut_ptr(), POWHASH_SIZE);
        }

        let hash = self.hasher.finalize_with_nonce(nonce);
        let hash = job.matrixhasher.0.heavy_hash(hash);

        Solution {
            id: 0,
            target: Uint256::from_le_bytes(hash.as_bytes()),
            nonce,
        }
    }
    pub fn compute(&mut self, job: &Job, nonce: u64) -> Option<Solution> {
        let mut solution = self.compute_raw(job, nonce);

        // info!("nonce: {}, diff: {}", nonce, target2difficulty(&solution.target));
        if solution.target <= job.target {
            solution.id = atomic_id();
            Some(solution)
        } else {
            None
        }
    }
}

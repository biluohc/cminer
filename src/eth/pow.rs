use bigint::{H256, H64, U256};
use bytesize::ByteSize;
use rayon::current_num_threads;
use std::sync::Arc;

use crate::eth::proto::{Job, Solution};
use crate::util::atomic_id;

pub fn target_to_difficulty(target: &H256) -> U256 {
    let d = U256::from(target);
    if d <= U256::one() {
        U256::max_value()
    } else {
        ((U256::one() << 255) / d) << 1
    }
}

/// Convert an Ethash difficulty to the target. Basically just `f(x) = 2^256 / x`.
pub fn difficulty_to_target(difficulty: &U256) -> H256 {
    if *difficulty <= U256::one() {
        U256::max_value().into()
    } else {
        (((U256::one() << 255) / *difficulty) << 1).into()
    }
}

use digest::Digest;
use sha3::Keccak256;
pub fn get_epoch_number(seed_hash: &H256) -> Result<usize, ()> {
    let mut epoch = 0;
    let mut seed = [0u8; 32];
    while seed != seed_hash[..] {
        let mut hasher = Keccak256::default();
        hasher.input(&seed);
        let output = hasher.result();
        for i in 0..32 {
            seed[i] = output[i];
        }
        epoch += 1;
        if epoch > 10000 {
            eprintln!("Failed to determin epoch");
            return Err(());
        }
    }
    Ok(epoch)
}

use std::cell::UnsafeCell;
unsafe impl Sync for FullBytes {}
pub struct FullBytes {
    size: usize,
    bytes: UnsafeCell<Vec<u8>>,
}

impl FullBytes {
    pub fn new(size: usize) -> Self {
        let mut bytes = Vec::with_capacity(size);
        bytes.resize(size, 0u8);
        Self { size, bytes: UnsafeCell::from(bytes) }
    }
    pub fn as_mut_bytes(&self) -> &mut [u8] {
        unsafe { self.bytes.get().as_mut().unwrap().as_mut_slice() }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &*self.as_mut_bytes()
    }
    pub fn size(&self) -> usize {
        self.size
    }
}

#[derive(Clone)]
pub struct Computer {
    epoch: usize,
    full: Arc<FullBytes>,
}

use std::fmt;
impl fmt::Debug for Computer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {}, {})", self.epoch, self.size(), ByteSize(self.size() as _))
    }
}

impl Computer {
    pub fn new(epoch: usize) -> Self {
        warn!("Computer::new, epoch: {}, current_num_threads: {}", epoch, current_num_threads());
        let light_size = ethash::get_cache_size(epoch);
        let full_size = ethash::get_full_size(epoch);
        // trace!("light-size: {}, {}", light_size, ByteSize::b(light_size as _));
        info!("full_-size: {}, {}", full_size, ByteSize::b(full_size as _));

        let mut light = Vec::with_capacity(light_size);
        light.resize(light_size, 0u8);
        ethash::make_cache(&mut light, ethash::get_seedhash(epoch));
        let light = Arc::from(light);

        let full = Arc::from(FullBytes::new(full_size));
        make_full(&full, &light);
        warn!("Computer::new ok, epoch: {}, current_num_threads: {}", epoch, current_num_threads());

        Self { epoch, full }
    }
    pub fn epoch(&self) -> usize {
        self.epoch
    }
    pub fn size(&self) -> usize {
        self.full.size()
    }
    pub fn compute(&self, job: &Job, nonce: u64) -> Option<Solution> {
        let mut solution = self.compute_raw(job, nonce);

        // info!("nonce: {}, diff: {}", nonce, target_to_difficulty(&solution.target));
        if solution.target <= job.target {
            solution.id = atomic_id();
            Some(solution)
        } else {
            None
        }
    }
    pub fn compute_raw(&self, job: &Job, nonce: u64) -> Solution {
        let full = self.full.as_bytes();

        let (mixed_hash, result) = ethash::hashimoto_full(job.powhash, H64::from(nonce), full.len(), full);

        Solution {
            id: job.id,
            target: result,
            mixed_hash,
            nonce,
        }
    }
}

use rayon::prelude::*;
/// unsafe impl for https://docs.rs/ethash/0.3.1/src/ethash/lib.rs.html#176-184
pub fn make_full(full: &Arc<FullBytes>, cache: &Arc<Vec<u8>>) {
    const HASH_BYTES: usize = 64;

    let dataset = full.as_bytes();
    let n_scope = dataset.len() / HASH_BYTES;
    let n_worker = current_num_threads();
    let n_task = n_scope / n_worker;
    let tasks = (0..n_scope).into_iter().collect::<Vec<_>>();
    let tasks = tasks.chunks(n_task).map(|ts| (ts, full.clone(), cache.clone())).collect::<Vec<_>>();

    tasks.into_par_iter().for_each(move |(tasks, full, cache)| {
        let dataset = full.as_mut_bytes();
        for i in tasks {
            let z = ethash::calc_dataset_item(&cache, *i);
            for j in 0..64 {
                dataset[i * 64 + j] = z[j];
            }
        }
    })
}

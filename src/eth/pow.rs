use bigint::{BigEndianHash, H256, H64, U64};
use bytesize::ByteSize;
use rayon::current_num_threads;
use std::sync::Arc;

use crate::eth::proto::{FormJob, Job, Solution};
use crate::util::{atomic_id, target_to_difficulty};

pub fn fun() {
    let notify = r#"{"id":0,"jsonrpc":"2.0","result":["0x93cca7a948af373321f5ba7a5de6b51d60348afd86063fbddd7dc4e553560798","0x1a7d0730fc4d6e634f5506e6530175aaea40fddd86fa7d41af81ef34f7293b09","0x000001ad7f29abcaf485787a6520ec08d23699194119a5c37387b71906614310"]}"#;
    let jobform: FormJob = serde_json::from_str(notify).unwrap();
    let job = jobform.to_job().unwrap();

    info!("epoch: {}", job.epoch);
    let computer = Computer::new(job.epoch, 1, false);

    let now = std::time::Instant::now();
    let mut nonce = 0.into();
    loop {
        nonce = nonce + 1;

        let solution = computer.compute_raw(&job, &nonce);

        info!(
            "ph: {}, nonce: {}, diff: {}, result: {}, mix: {}",
            job.powhash,
            nonce,
            target_to_difficulty(&solution.target),
            solution.target,
            solution.mixed_hash
        );

        if nonce == 1_000_000.into() {
            break;
        }
    }
    info!("1m {:?}, {} hash/s", now.elapsed(), nonce / now.elapsed().as_secs());
}

use digest::Digest;
use sha3::Keccak256;
pub fn get_epoch_number(seed_hash: &H256) -> Result<usize, ()> {
    let mut epoch = 0;
    let mut seed = [0u8; 32];
    while seed != seed_hash[..] {
        let mut hasher = Keccak256::default();
        hasher.update(&seed);
        let output = hasher.finalize();
        for i in 0..32 {
            seed[i] = output[i];
        }
        epoch += 1;
        if epoch > 10000 {
            error!("failed to determin epoch: {:?}", seed_hash);
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
        let bytes = vec![0u8; size];
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
    // ethash epoch_length is 30000
    // etchash epoch_length is 60000
    pub fn new(mut epoch: usize, wokrers: usize, etc: bool) -> Self {
        if etc {
            epoch /= 2;
        }
        let light_size = ethash::get_cache_size(epoch);
        let full_size = ethash::get_full_size(epoch);
        warn!(
            "Computer::new, epoch: {}, light: {}, full: {}, current_num_threads: {}",
            epoch,
            ByteSize::b(light_size as _),
            ByteSize::b(full_size as _),
            current_num_threads()
        );

        let mut full = Arc::from(FullBytes::new(0));
        if wokrers > 0 {
            let mut light = vec![0; light_size];
            ethash::make_cache(&mut light, ethash::get_seedhash(if etc { epoch * 2 } else { epoch }));
            let light = Arc::from(light);

            full = Arc::from(FullBytes::new(full_size));
            make_full(&full, &light);

            warn!("Computer::new ok, epoch: {}", epoch);
        } else {
            error!("Computer::new skip, wokrers: {}", wokrers);
        };

        Self { epoch, full }
    }
    pub fn epoch(&self) -> usize {
        self.epoch
    }
    pub fn size(&self) -> usize {
        self.full.size()
    }
    pub fn compute(&self, job: &Job, nonce: &U64) -> Option<Solution> {
        let mut solution = self.compute_raw(job, nonce);

        // info!("nonce: {}, diff: {}", nonce, target_to_difficulty(&solution.target));
        if solution.target <= job.target {
            solution.id = atomic_id();
            Some(solution)
        } else {
            None
        }
    }
    pub fn compute_raw(&self, job: &Job, nonce: &U64) -> Solution {
        let full = self.full.as_bytes();
        let nonce = H64::from_uint(nonce);

        let (mixed_hash, result) = ethash::hashimoto_full(job.powhash, nonce, full.len(), full);

        Solution {
            id: job.id,
            target: result,
            mixed_hash,
            nonce,
        }
    }
}

use rayon::prelude::*;

/// a parallel implementation for https://docs.rs/ethash/0.3.1/src/ethash/lib.rs.html#176-184
/// it will take more than 40 minutes if using single thread only
pub fn make_full(full: &Arc<FullBytes>, cache: &Arc<Vec<u8>>) {
    const HASH_BYTES: usize = 64;

    let dataset = full.as_bytes();
    let n_scope = dataset.len() / HASH_BYTES;
    let n_worker = current_num_threads();
    let n_task = n_scope / n_worker;
    let tasks = (0..n_scope).collect::<Vec<_>>();
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

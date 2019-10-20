#[macro_use]
pub extern crate nonblock_logger;

fn main() {
    let _handle = nonblock_logger::NonblockLogger::new()
        .log_to_stdout()
        .map_err(|e| eprintln!("failed to init nonblock_logger: {:?}", e))
        .unwrap();

    fun()
}

fn fun() {
    let powhash = "0xf13161f2a062780bad5f2231d9accb64aef699483ca1356fb82e6a36f782ad3f";
    let seedhash = "0x1a7d0730fc4d6e634f5506e6530175aaea40fddd86fa7d41af81ef34f7293b09";
    let target = "0x000001ad7f29abcaf485787a6520ec08d23699194119a5c37387b71906614310";
    info!("powhash: {}", powhash);
    info!("sedhash: {}", seedhash);
    info!("target: {}", target);

    let seed_hash = clean_0x(seedhash).parse().unwrap();
    let epoch = get_epoch_number(&seed_hash).unwrap();
    info!("epoch: {}", epoch);

    let cache_size = ethash::get_cache_size(epoch);
    let full_size = ethash::get_full_size(epoch);
    info!("cache-size: {}", cache_size);
    info!("full_-size: {}", full_size);

    let mut cache = Vec::with_capacity(cache_size);
    cache.resize(cache_size, 0u8);
    ethash::make_cache(&mut cache, ethash::get_seedhash(epoch));

    info!("current_num_threads: {}", rayon::current_num_threads());
    let cache = Arc::from(cache);
    let full = Arc::from(FullBytes::new(full_size));
    make_full(&full, &cache);
    info!("make_full: {}", rayon::current_num_threads());

    let full = full.as_bytes();

    let pow_hash: H256 = clean_0x(powhash).parse().unwrap();
    let target_h256: H256 = clean_0x(target).parse().unwrap();

    let now = std::time::Instant::now();
    let mut nonce = 0;
    loop {
        nonce += 1;
        let (mixed_hash, result) = ethash::hashimoto_full(pow_hash, H64::from(nonce), full_size, full);

        // info!("nonce: {}, diff: {}", nonce, target_to_difficulty(&H256::from(result)));
        if result <= target_h256 {
            break;
        }
        if nonce == 1000_000 {
            break;
        }
    }
    info!("1m {:?}, {} hash/s", now.elapsed(), nonce / now.elapsed().as_secs());
}

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

use std::sync::Arc;

use bigint::{H256, H64, U256};
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

pub fn clean_0x(s: &str) -> &str {
    if s.starts_with("0x") {
        &s[2..]
    } else {
        s
    }
}


use rayon::prelude::*;
// https://docs.rs/ethash/0.3.1/src/ethash/lib.rs.html#176-184
pub fn make_full(full: &Arc<FullBytes>, cache: &Arc<Vec<u8>>) {
    const HASH_BYTES: usize = 64;

    let dataset = full.as_bytes();
    let n_scope = dataset.len() / HASH_BYTES;
    let n_worker = rayon::current_num_threads();
    let n_task = n_scope / n_worker;
    let tasks = (0..n_scope).into_iter().collect::<Vec<_>>();
    let tasks = tasks
        .chunks(n_task)
        .map(|ts| (ts, full.clone(), cache.clone()))
        .collect::<Vec<_>>();

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
        Self {
            size,
            bytes: UnsafeCell::from(bytes),
        }
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

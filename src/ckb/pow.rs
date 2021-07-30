use bigint::H256;
use eaglesong::eaglesong;
use faster_hex::hex_decode;

use crate::ckb::proto::{Job, Solution};
use crate::util::{atomic_id, target_to_difficulty};

pub type Cache = [u8; 48];
pub type Nonce = [u8; 16];
pub type Hash = [u8; 32];

#[derive(Clone)]
pub struct Computer {
    cache: Cache,
    testnet: bool,
}

// ckb testnet use eaglesong_blake2b
// https://github.com/nervosnetwork/ckb/blob/v0.37.0/pow/src/lib.rs#L21
impl Computer {
    pub fn new(testnet: bool) -> Self {
        Self { cache: [0u8; 48], testnet }
    }
    pub fn update(&mut self, powhash: &str) {
        hex_decode(powhash.as_bytes(), &mut self.cache[0..32]).expect("Computer.update");
    }
    pub fn compute_raw(&mut self, _job: &Job, nonce: u128) -> Solution {
        let nonce_bytes: Nonce = nonce.to_be_bytes();

        // (&mut self.cache[32..]).copy_from_slice(&nonce_bytes[..]);
        unsafe {
            std::ptr::copy_nonoverlapping((&nonce_bytes[..]).as_ptr(), (&mut self.cache[32..]).as_mut_ptr(), 16);
        }

        let mut hash: Hash = [0u8; 32];
        eaglesong(&self.cache[..], &mut hash[..]);

        if self.testnet {
            hash = ckb_hash::blake2b_256(&hash);
        }

        Solution { id: 0, nonce, target: hash.into() }
    }
    pub fn compute(&mut self, job: &Job, nonce: u128) -> Option<Solution> {
        let mut solution = self.compute_raw(job, nonce);

        // info!("nonce: {}, diff: {}", nonce, target_to_difficulty(&solution.target));
        if solution.target <= job.target {
            solution.id = atomic_id();
            Some(solution)
        } else {
            None
        }
    }
}

pub fn parse_nonce(nonce1: &str) -> (u128, usize) {
    let nonce1_bytes = nonce1.len() / 2;

    if nonce1_bytes > 16 || nonce1_bytes % 2 == 1 {
        fatal!("invalid nonce1: {}, len: {}, bytes: {}", nonce1, nonce1.len(), nonce1_bytes);
    }

    let mut nbs = [0u8; 16];
    hex_decode(nonce1.as_bytes(), &mut nbs[..nonce1_bytes]).expect("parse_nonce.hex_decode()");
    let nonce = u128::from_be_bytes(nbs);

    (nonce, nonce1_bytes)
}

pub fn fun() {
    let powhash = "e365d3112a76b706d8f89dbd6f1b7a80d9b3d8ab2eaa76f70d8d012caecc2ce8";
    let nonce = "e8ae6a1f0000000000000000003e6b8f";

    let mut input = [0u8; 48];
    hex_decode(powhash.as_bytes(), &mut input[0..32]).expect("ph");
    hex_decode(nonce.as_bytes(), &mut input[32..48]).expect("no");
    let mut nbs = [0u8; 16];
    hex_decode(nonce.as_bytes(), &mut nbs[..]).unwrap();
    let nonce_num = u128::from_le_bytes(nbs);
    println!("nonce_num: {}", nonce_num);

    let mut hash = [0u8; 32];
    eaglesong(&input[..], &mut hash[..]);

    println!("hash: {:?}", &hash[..]);
    println!("diff: {:?}", target_to_difficulty(&H256::from(hash)));
}

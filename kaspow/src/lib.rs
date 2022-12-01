#[macro_use]
extern crate anyhow;

// public for benchmarks
#[doc(hidden)]
pub mod matrix;
#[doc(hidden)]
pub mod xoshiro;

use crate::matrix::Matrix;
use math::Uint320;
use std::mem;

pub use hashes::{Hash, PowHash};
pub use math::Uint256;

pub fn target2difficulty(target: &Uint256) -> Uint256 {
    if *target == Uint256::from(1) {
        return Uint256::MAX;
    }

    // le_bytes
    let mut target320 = Uint320::ZERO;
    target320.0[..4].copy_from_slice(&target.0);

    let res = Uint320::from_u64(1).wrapping_shl(256) / target320;
    Uint256(res.0[..4].try_into().unwrap())
}

pub fn difficulty_decompress(d: f64) -> u64 {
    (d * 2f64.powi(32)) as _
}

/// State is an intermediate data structure with pre-computed values to speed up mining.
pub struct State {
    pub matrix: Matrix,
    pub target: Uint256,
    // PRE_POW_HASH || TIME || 32 zero byte padding; without NONCE
    pub hasher: PowHash,
}

impl State {
    // #[inline]
    // pub fn new(header: &Header) -> Self {
    //     let target = Uint256::from_compact_target_bits(header.bits);
    //     // Zero out the time and nonce.
    //     let pre_pow_hash = hashing::header::hash_override_nonce_time(header, 0, 0);
    //     // PRE_POW_HASH || TIME || 32 zero byte padding || NONCE
    //     let hasher = PowHash::new(pre_pow_hash, header.timestamp);
    //     let matrix = Matrix::generate(pre_pow_hash);

    //     Self { matrix, target, hasher }
    // }
    pub fn with_powhash(powhash: &str, diff: u64) -> anyhow::Result<Self> {
        const SIZE: usize = mem::size_of::<Hash>() * 2;

        let hash = powhash[0..SIZE].parse::<Hash>().map_err(|e| format_err!("powhash.pre parse failed: {}", e))?;

        let mut timestamp = [0u8; 8];
        faster_hex::hex_decode(&powhash.as_bytes()[SIZE..], &mut timestamp).map_err(|e| format_err!("powhash.timestamp parse failed: {}", e))?;
        let timestamp = u64::from_le_bytes(timestamp);

        Self::with_prehash_timestamp(hash, timestamp, diff)
    }

    #[inline]
    pub fn with_prehash_timestamp(hash: Hash, timestamp: u64, diff: u64) -> anyhow::Result<Self> {
        let diff = Uint256::from_u64(diff);
        let target = target2difficulty(&diff);

        let hasher = PowHash::new(hash, timestamp);
        let matrix = Matrix::generate(hash);

        Ok(Self { matrix, hasher, target })
    }

    #[inline]
    #[must_use]
    /// PRE_POW_HASH || TIME || 32 zero byte padding || NONCE
    pub fn calculate_pow(&self, nonce: u64) -> Uint256 {
        // Hasher already contains PRE_POW_HASH || TIME || 32 zero byte padding; so only the NONCE is missing
        let hash = self.hasher.clone().finalize_with_nonce(nonce);
        let hash = self.matrix.heavy_hash(hash);
        Uint256::from_le_bytes(hash.as_bytes())
    }

    #[inline]
    #[must_use]
    pub fn check_pow(&self, nonce: u64) -> (bool, Uint256) {
        let pow = self.calculate_pow(nonce);
        // The pow hash must be less or equal than the claimed target.
        (pow <= self.target, pow)
    }
}

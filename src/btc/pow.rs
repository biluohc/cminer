use bitcoin::consensus::encode::{Decodable, Encodable};
use bitcoin::util::uint::Uint256;
use bitcoin::BlockHeader;
use bitcoin::Transaction;
use bitcoin::TxMerkleNode;
use bitcoin_hashes::sha256d;
use bitcoin_hashes::sha256d::Hash;
use bitcoin_hashes::Hash as HashTrait;
use bytes::buf::ext::BufExt;
use hex::decode;

use std::collections::VecDeque;
use std::convert::TryInto;

use super::proto::{Job, Solution};
use crate::util::atomic_id;

pub type HashRaw = [u8; 32];

pub fn cal_merkle_root_by_branch(mut tx_hash: VecDeque<HashRaw>) -> Hash {
    let mut data = Vec::with_capacity(32 * 2);
    while tx_hash.len() > 1 {
        let first = tx_hash.pop_front().unwrap();
        let second = tx_hash.pop_front().unwrap();

        data.clear();
        data.extend_from_slice(&first);
        data.extend_from_slice(&second);

        let hash = sha256d::Hash::hash(&data[..]);
        let buf = hash.into_inner();
        tx_hash.push_front(buf);
    }

    assert_eq!(1, tx_hash.len());
    let root = tx_hash.pop_front().unwrap();
    Hash::from_inner(root)
}

fn assemble_coinbase_tx(coinbase1: &[u8], coinbase2: &[u8], extra_nonce1: &[u8], extra_nonce2: &[u8]) -> Vec<u8> {
    let mut c = Vec::with_capacity(coinbase1.len() + coinbase2.len() + extra_nonce1.len() + extra_nonce2.len());
    c.extend_from_slice(coinbase1);
    c.extend_from_slice(extra_nonce1);
    c.extend_from_slice(extra_nonce2);
    c.extend_from_slice(coinbase2);
    c
}

pub fn coinbase_for_block(coinbase1: &[u8], coinbase2: &[u8], extra_nonce1: &[u8], extra_nonce2: &[u8]) -> Result<Transaction, crate::util::Error> {
    let coinbase = assemble_coinbase_tx(coinbase1, coinbase2, extra_nonce1, extra_nonce2);
    let r = std::io::Cursor::new(coinbase).reader();
    let mut coinbase_tx: Transaction = Decodable::consensus_decode(r)?;

    // add witness
    let i = &mut coinbase_tx.input[0];
    i.witness = vec![vec![0u8; 32]];

    Ok(coinbase_tx)
}

#[inline]
pub fn target_uint256_from_hashraw(d: &HashRaw) -> Uint256 {
    let mut out = Uint256([0u64; 4]);

    for (i, dd) in (&d[..]).chunks(8).enumerate() {
        out.0[i] = u64::from_le_bytes(dd.try_into().unwrap())
    }

    out
}

#[inline]
pub fn target_uint256_from_hashraw_origin(d: &HashRaw) -> Uint256 {
    Decodable::consensus_decode(&d[..]).unwrap()
}

// cargo tr btc_tar -- --nocapture
#[test]
fn btc_target() {
    let b = false;
    let times = 10000_0000;

    for _ in 0..times {
        let d = rand::random();

        let origin = target_uint256_from_hashraw_origin(&d);
        let raw = target_uint256_from_hashraw(&d);
        // println!("dd: {:?}, \ndo: {}\ndr: {:?}", d, origin, raw);

        assert_eq!(origin, raw);
    }

    fn bench<F: Fn(u32)>(tag: &str, times: u32, f: F) {
        use std::time::Instant;

        let now = Instant::now();
        (0..times).into_iter().for_each(|c| f(c));
        let costed = now.elapsed();
        println!("{} {} times costed {:?}, avg time: {:?}", tag, times, costed, costed / times)
    }

    bench("raw_", times, |_| {
        let a = rand::random();
        let d = target_uint256_from_hashraw(&a);
        if b {
            println!("{}: {:?}", d, a);
        }
    });

    bench("orig", times, |_| {
        let a = rand::random();
        let d = target_uint256_from_hashraw_origin(&a);
        if b {
            println!("{}: {:?}", d, a);
        }
    });

    let unit = unit_target();
    bench("raw_cmp_skip", times, |_| {
        let a = rand::random();
        let d = target_uint256_from_hashraw(&a);

        // small pool diff 1 = 2^(8*4) > 4.2 G
        if &a[28..] == &[0, 0, 0, 0] && d <= unit && b {
            println!("{}: {:?}", d, a);
        }
    });

    bench("raw_cmp_dire", times, |_| {
        let a = rand::random();
        let d = target_uint256_from_hashraw(&a);
        if d <= unit && b {
            println!("{}: {:?}", d, a);
        }
    });
}
/*
i: 0x000000000000000000000000000000000000000000000000000000000000ffff
f: 0x00000000ffff0000000000000000000000000000000000000000000000000000
52 (0xf 2^4) => 26 (0xff 2^8)
*/
#[inline]
pub fn unit_target() -> Uint256 {
    Uint256::from_u64(0xFFFF).unwrap() << (52 * 4)
}

#[inline]
pub fn target_to_difficulty(target: &Uint256) -> u64 {
    (unit_target() / *target).low_u64()
}

#[derive(Clone)]
pub struct Computer {
    bytes: [u8; 80],
}

impl Computer {
    pub fn new() -> Self {
        Self { bytes: [0; 80] }
    }
    pub fn update(&mut self, job: &Job) {
        let nonce1 = decode(&job.nonce1).unwrap();
        let nonce2 = job.nonce2_bytes();
        let txid = coinbase_for_block(job.coinbase_part1.as_slice(), job.coinbase_part2.as_slice(), nonce1.as_slice(), nonce2.as_slice())
            .unwrap()
            .txid();
        let mut merkle_branchs = job.merkle_branches.clone();
        merkle_branchs.push_front(txid.as_hash().into_inner());

        let merkle_root = cal_merkle_root_by_branch(merkle_branchs);
        let merkle_root: TxMerkleNode = merkle_root.into();

        let header = BlockHeader {
            merkle_root,
            time: job.ntime,
            bits: job.nbits,
            version: job.version,
            prev_blockhash: job.prev_hash.into(),
            nonce: 0,
        };

        let mut encoder = std::io::Cursor::new(vec![]);
        header.consensus_encode(&mut encoder).unwrap();
        let header_bytes = encoder.into_inner();
        self.bytes.clone_from_slice(header_bytes.as_slice());
    }

    #[inline]
    pub fn compute_origin(&mut self, job: &Job, nonce: u32) -> Option<Solution> {
        use bitcoin::hash_types::BlockHash;
        use std::io::Write;

        let bytes = &mut self.bytes;
        (&mut bytes[76..]).write_all(&nonce.to_le_bytes()).unwrap();
        let hash = BlockHash::hash(&bytes[..]);
        let hashraw = hash.into_inner();

        let target = target_uint256_from_hashraw(&hashraw);
        if target <= job.target {
            return Some(Solution { target, nonce, id: atomic_id() });
        }

        None
    }

    // ring is faster
    #[inline]
    pub fn compute(&mut self, job: &Job, nonce: u32) -> Option<Solution> {
        use ring::digest;
        use std::io::Write;

        let bytes = &mut self.bytes;
        (&mut bytes[76..]).write_all(&nonce.to_le_bytes()).unwrap();

        let hash = digest::digest(&digest::SHA256, digest::digest(&digest::SHA256, bytes).as_ref());
        let hashraw: &HashRaw = TryInto::try_into(hash.as_ref()).unwrap();

        let target = target_uint256_from_hashraw(&hashraw);
        if target <= job.target {
            return Some(Solution { target, nonce, id: atomic_id() });
        }

        None
    }
}

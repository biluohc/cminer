use bitcoin::util::uint::Uint256;
use bitcoin_hashes::sha256d::Hash;

use faster_hex::{hex_decode, hex_string};
use futures::future::Either;
use serde_json::Value;

use std::collections::VecDeque;

use super::pow::HashRaw;
use crate::config::Config;
use crate::state::Req;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodForm {
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
    pub error: Option<Value>,
}

// {"id":null,"method":"mining.set_difficulty","params":[1]}
pub type FormSetDifficulty = (u64,);

/*
{
    "params": [
        "Jy4KZryMoz0=",
        "02391661d953700a84f962ca118eb3226a786a9f2018264d000013cc00000000",
        "02000000010000000000000000000000000000000000000000000000000000000000000000ffffffff1603fb341a045d81965e0c",
        "ffffffff02166d2b01000000001976a91424c49aeb43d0ddcb3529cbbaf05770d9f435170388ac0000000000000000266a24aa21a9edd984ce3250f03b34be92e233252d08975a6a16f1cb63265aa6ddc6cab5efbc8300000000",
        [
            "2c4400a727316979f03161851244f7bd0e67e040b8f3806cd86f44342164fc92",
            "91148d63cc82e6a0f435c3c1820974b22424d70c658944f25955473d23c854bd",
            "8f89a6545e1ebe62f058e19b8ef87c1d085261efe3ee8026cd2fd66ab019dace"
        ],
        "20000000",
        "1a2a7f80",
        "5e968131",
        false
    ],
    "id": null,
    "method": "mining.notify"
}
*/
pub type FormJob = (String, String, String, String, Vec<String>, String, String, String, bool);

pub fn parse_job(form: FormJob) -> Result<Job, crate::util::Error> {
    let (jobid, phash, txp1, txp2, branches, version, nbits, ntime, clean) = form;

    let mut bytes = [0u8; 4];
    hex_decode(version.as_bytes(), &mut bytes)?;
    let version = i32::from_be_bytes(bytes);
    hex_decode(nbits.as_bytes(), &mut bytes)?;
    let nbits = u32::from_be_bytes(bytes);
    hex_decode(ntime.as_bytes(), &mut bytes)?;
    let ntime = u32::from_be_bytes(bytes);

    let phash: String = phash.chars().collect::<Vec<char>>().chunks(8).rev().flatten().collect();
    let prev_hash = phash.parse::<Hash>()?.into();
    let coinbase_part1 = hex::decode(&txp1)?;
    let coinbase_part2 = hex::decode(&txp2)?;
    let mut merkle_branches = VecDeque::with_capacity(branches.len() + 1);
    let mut bytes = [0u8; 32];
    for b in branches {
        hex_decode(b.as_bytes(), &mut bytes)?;
        merkle_branches.push_back(bytes);
    }

    Ok(Job {
        id: 0,
        nonce2: 0,
        nonce2_max: 0,
        nonce2_bytes: 0,
        target: Default::default(),
        nonce1: Default::default(),
        jobid,
        version,
        nbits,
        ntime,
        clean,
        merkle_branches,
        prev_hash,
        coinbase_part1,
        coinbase_part2,
    })
}

impl MethodForm {
    pub fn to_params(self) -> Result<Either<Job, u64>, crate::util::Error> {
        let method = self.method.as_str();
        if method == METHOD_NOTIFY {
            serde_json::from_value(self.params).map_err(Into::into).and_then(|p: FormJob| parse_job(p).map(Either::Left))
        } else if method == METHOD_SET_TARGET {
            serde_json::from_value(self.params).map_err(Into::into).map(|p: FormSetDifficulty| Either::Right(p.0))
        } else {
            Err(format_err!("unkown MethodForm"))
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub struct Job {
    pub id: usize,
    pub jobid: String,
    pub target: Uint256,
    pub nonce1: String,
    pub nonce2: u128,
    pub nonce2_max: u128,
    pub nonce2_bytes: usize,
    pub nbits: u32,
    pub ntime: u32,
    pub version: i32,
    pub coinbase_part1: Vec<u8>,
    pub coinbase_part2: Vec<u8>,
    pub merkle_branches: VecDeque<HashRaw>,
    pub prev_hash: Hash,
    pub clean: bool,
}

impl Job {
    pub fn nonce2_bytes(&self) -> Vec<u8> {
        let nonce2 = self.nonce2.to_le_bytes();
        nonce2[..self.nonce2_bytes].to_vec()
    }
}

#[derive(Debug, Clone)]
pub struct Solution {
    pub id: usize,
    pub target: Uint256,
    pub nonce: u32,
}

pub const METHOD_SUBSCRIBE: &str = "mining.subscribe";
pub const METHOD_AUTHORIZE: &str = "mining.authorize";
pub const METHOD_SET_TARGET: &str = "mining.set_difficulty";
pub const METHOD_NOTIFY: &str = "mining.notify";
pub const METHOD_SUBMIT_WORK: &str = "mining.submit";

// [user, jobid, nonce2, ntime, nonce]
// {"method": "mining.submit", "params": ["sp_yos.cpux", "Jy4KZryMoz0=", "0400000000000000", "5e968131", "24b9d8f9"], "id":8}
// {"id":8,"result":true,"error":null}
pub fn make_submit(solution: &Solution, job: &Job) -> Option<Req> {
    let nonce2 = job.nonce2_bytes();
    let nonce2_submit = hex_string(nonce2.as_slice());

    let ntime = job.ntime.to_be_bytes();
    let ntime_submit = hex_string(ntime.as_ref());

    let nonce = solution.nonce.to_be_bytes();
    let nonce_submit = hex_string(nonce.as_ref());

    let req = format!(
        r#"{{"id":{},"method":"{}","params":["{}", "{}", "{}", "{}", "{}"]}}"#,
        solution.id, METHOD_SUBMIT_WORK, "", job.jobid, nonce2_submit, ntime_submit, nonce_submit
    );
    Some((solution.id, METHOD_SUBMIT_WORK, req).into())
}

// r: {"id": 1, "method": "mining.subscribe", "params": ["cpuminer/2.5.0"]}
// p: {"id":1,"result":[[["mining.notify","ca53a260"]],"ca53a260",8],"error":null}
// [ [["", session id]], "nonce1", nonce2-bytes ]

// r: {"id": 2, "method": "mining.authorize", "params": ["sp_yos.cpux", ""]}
// p: {"id":2,"result":true,"error":null}
pub fn make_login(config: &Config) -> Req {
    let login = format!(
        r#"{{"id":0,"method":"{}","params":["{}-{}",null]}}
        {{"id":0,"method":"{}","params":["{}.{}","x"]}}"#,
        METHOD_SUBSCRIBE,
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        METHOD_AUTHORIZE,
        config.user,
        config.worker
    );
    (0, METHOD_SUBSCRIBE, login).into()
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultForm {
    pub id: usize,
    pub result: Value,
    pub error: Option<Value>,
}

impl ResultForm {
    // <(id, bool, _), (nonce1, nonce2, _)>
    pub fn to_result(&self) -> Result<Either<(usize, bool, Option<String>), (String, usize, Option<String>)>, &'static str> {
        if let Ok(b) = serde_json::from_value::<bool>(self.result.clone()) {
            return Ok(Either::Left((self.id, b, self.error.as_ref().map(|e| format!("{:?}", e)))));
        }
        if let Ok((_, nonce1, nonce2)) = serde_json::from_value::<(Value, String, usize)>(self.result.clone()) {
            Ok(Either::Right((nonce1, nonce2, self.error.as_ref().map(|e| format!("{:?}", e)))))
        } else {
            Err("Invalid ResultForm")
        }
    }
}

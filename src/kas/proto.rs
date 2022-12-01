use faster_hex::hex_string;
use kaspow::{difficulty_decompress, matrix::Matrix, target2difficulty, Hash, PowHash, Uint256};
use serde_json::Value;
use std::sync::Arc;

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

// {"id":null,"jsonrpc":"2.0","method":"mining.set_difficulty","params":[2.3283064365386963]}
pub type FormSetDifficulty = (f64,);

// {"id": 3, "method": "mining.set_extranonce", "params": ["3d9e"]}
// {"jsonrpc":"2.0","method":"mining.set_extranonce","params":["5772",6],"id":null}
// acc-pool?: {"id": 0, "method": "set_extranonce", "params": ["c5ec", 6], "error": null}
// pub type FormSetExtranonce = Value;

// {"id":null,"method":"mining.notify","params":["6d4247","6d424711c4474660a970ab4c63452b46d7bc468c91766917c458496a8d3994fce15aaecb84010000"]}
// {"id":null,"method":"mining.notify","params":["e05d67",[7581372481393876448,16082693024256892755,12921235528852832060,12942364641311238],1669864495287]}
pub type FormJobHex = (String, String);
pub type FormJob = (String, [u64; 4], u64);

pub fn parse_job(js: Value) -> Result<Job, &'static str> {
    let e = "invalid mining.notify params";
    let err = Err(e);
    if !js.is_array() {
        return err;
    }

    let array = js.as_array().unwrap();

    let jobid;
    let powhash: Hash;
    let timestamp: u64;
    match array.len() {
        2 => {
            let job: FormJobHex = serde_json::from_value(js.clone()).map_err(|_| e)?;

            jobid = job.0;

            let hex = &job.1;
            if hex.len() < 80 {
                return err;
            }

            powhash = (&hex[..64]).parse().map_err(|_| e)?;
            let mut bytes = [0u8; 8];
            faster_hex::hex_decode(&hex.as_bytes()[64..], &mut bytes).map_err(|_| e)?;
            timestamp = u64::from_le_bytes(bytes);
        }
        3 => {
            let job: FormJob = serde_json::from_value(js.clone()).map_err(|_| e)?;
            jobid = job.0;
            powhash = Hash::from_le_u64(job.1);
            timestamp = job.2;
        }
        _ => return err,
    }

    let hasher = PowHash::new(powhash, timestamp);
    let matrix = Matrix::generate(powhash);
    let matrixhasher = Arc::new((matrix, hasher));

    Ok(Job {
        jobid,
        powhash,
        matrixhasher,
        timestamp,
        nonce1_bytes: 0,
        target: Default::default(),
        nonce: 0,
        id: 0,
    })
}

pub fn parse_nonce(nonce1: &str) -> (u64, usize) {
    let nonce1_bytes = nonce1.len() / 2;

    if nonce1_bytes > 16 || nonce1_bytes % 2 == 1 {
        fatal!("invalid nonce1: {}, len: {}, bytes: {}", nonce1, nonce1.len(), nonce1_bytes);
    }

    let mut nbs = [0u8; 8];
    faster_hex::hex_decode(nonce1.as_bytes(), &mut nbs[..nonce1_bytes]).expect("parse_nonce.hex_decode()");
    let nonce = u64::from_be_bytes(nbs);

    (nonce, nonce1_bytes)
}

impl MethodForm {
    pub fn to_params(self) -> Result<MethodParams, &'static str> {
        let method = self.method.as_str();
        if method == METHOD_NOTIFY {
            parse_job(self.params).map(MethodParams::Job)
        } else if method == METHOD_SET_TARGET {
            serde_json::from_value(self.params).map_err(|_| "deser_difficulty error").and_then(|p: FormSetDifficulty| {
                let diff = difficulty_decompress(p.0);
                let target = target2difficulty(&diff.into());

                info!("{} {}: {} target: {}", METHOD_SET_TARGET, p.0, diff, target);
                Ok(MethodParams::Target(target))
            })
        } else if [METHOD_SET_EXTRANONCE, "set_extranonce"].contains(&method) {
            let hex = self.params.as_array().and_then(|a| a.get(0)).and_then(|s| s.as_str());
            if hex.is_none() {
                return Err("malform set_extranonce");
            }
            let info = parse_nonce(hex.unwrap());
            info!("{} {}: {} {}bytes", METHOD_SET_EXTRANONCE, hex.unwrap(), info.0, info.1);
            Ok(MethodParams::Nonce1t(info))
        } else {
            Err("unkown MethodForm")
        }
    }
}

#[derive(Debug, Clone)]
pub enum MethodParams {
    Job(Job),
    Target(Uint256),
    Nonce1t((u64, usize)),
}

#[derive(Clone)]
pub struct Job {
    pub id: usize,
    pub jobid: String,
    pub powhash: Hash,
    pub target: Uint256,
    pub timestamp: u64,
    pub nonce: u64,
    pub nonce1_bytes: usize,
    pub matrixhasher: Arc<(Matrix, PowHash)>,
}

use std::fmt;
impl fmt::Debug for Job {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Job")
            .field("id", &self.id)
            .field("jobid", &self.jobid)
            .field("powash", &self.powhash)
            .field("target", &self.target)
            .field("timestamp", &self.timestamp)
            .field("nonce", &self.nonce)
            .field("nonce1_bytes", &self.nonce1_bytes)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct Solution {
    pub id: usize,
    pub target: Uint256,
    pub nonce: u64,
}

pub const METHOD_SUBSCRIBE: &str = "mining.subscribe";
pub const METHOD_AUTHORIZE: &str = "mining.authorize";
pub const METHOD_SET_TARGET: &str = "mining.set_difficulty";
pub const METHOD_SET_EXTRANONCE: &str = "mining.set_extranonce";
pub const METHOD_SUBMIT_HASHRATE: &str = "mining.submit_hashrate";
pub const METHOD_NOTIFY: &str = "mining.notify";
pub const METHOD_SUBMIT_WORK: &str = "mining.submit";

// {"id":8,"method":"mining.submit","params":["sp_test.worker1","b1404ef2","000000000000000000084148"]}
// {"id":8,"result":true,"error":null}
pub fn make_submit(solution: &Solution, job: &Job) -> Option<Req> {
    let nonce_bytes = solution.nonce.to_be_bytes();
    let nonce_submit = hex_string(&nonce_bytes);

    let req = format!(r#"{{"id":{},"method":"{}","params":["{}","{}","{}"]}}"#, solution.id, METHOD_SUBMIT_WORK, "", job.jobid, nonce_submit);
    Some((solution.id, METHOD_SUBMIT_WORK, req).into())
}

// {"id":9,"method":"mining.submit_hashrate","jsonrpc":"2.0","worker":"456-027","params":["0x000000000000000000000000ab5d1ce0","0xf3369d5a95fb31e9217f03484be600135c6c8250341ac4e7212269292e3ceb84"]}
pub fn make_hashrate(hashrate: u64) -> Req {
    let req = format!(
        r#"{{"jsonrpc":"2.0", "method":"{}", "params":["{:#0x}", "0x0000000000000000000000000000000000000000000000000000000000000000"],"id":1}}"#,
        METHOD_SUBMIT_HASHRATE, hashrate
    );
    (1, METHOD_SUBMIT_HASHRATE, req).into()
}

// r: {"id":1,"method":"mining.subscribe","params":["BzMiner/v12.1.1","EthereumStratum/1.0.0"]}
// p: {"id":1,"result":[true,"EthereumStratum/1.0.0"],"error":null}

// r: {"id":0,"method":"mining.authorize","params":["sp_test.worker1","x"]}
// p: {"id":0,"result":true,"error":null}
pub fn make_login(config: &Config) -> Req {
    let mut notify_hex = "";
    if config.testnet {
        notify_hex = ".BzMinerLike";
    }

    let login = format!(
        r#"{{"id":0,"method":"{}","params":["{}{}/{}","EthereumStratum/1.0.0"]}}
{{"id":1,"method":"{}","params":["{}.{}","x"]}}"#,
        METHOD_SUBSCRIBE,
        env!("CARGO_PKG_NAME"),
        notify_hex,
        env!("CARGO_PKG_VERSION"),
        METHOD_AUTHORIZE,
        config.user,
        config.rig
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
    // <(id, bool, _)
    pub fn to_result(&self) -> Result<(usize, bool, Option<String>), &'static str> {
        if let Ok((b, s)) = serde_json::from_value::<(bool, String)>(self.result.clone()) {
            info!("{} {}: {}", METHOD_SUBSCRIBE, b, s);
            return Ok((self.id, b, self.error.as_ref().map(|e| format!("{:?}", e))));
        }

        if let Ok(b) = serde_json::from_value::<bool>(self.result.clone()) {
            return Ok((self.id, b, self.error.as_ref().map(|e| format!("{:?}", e))));
        }

        Err("Invalid ResultForm")
    }
}

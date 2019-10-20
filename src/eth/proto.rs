use serde_json::Value;

// {"id":0,"jsonrpc":"2.0","result":["0x6c9e0bfc36b543a626c0d161d263a24df21c97956e665f87389dcc5cd908fedc","0x1a7d0730fc4d6e634f5506e6530175aaea40fddd86fa7d41af81ef34f7293b09","0x000001ad7f29abcaf485787a6520ec08d23699194119a5c37387b71906614310"]}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormJob {
    pub id: usize,
    pub jsonrpc: String,
    pub result: (String, String, String),
}

impl FormJob {
    pub fn to_job(&self) -> Result<Job, &'static str> {
        let seedhash = clean_0x(&self.result.1).parse().map_err(|_| "get seedhash error")?;
        Ok(Job {
            powhash: clean_0x(&self.result.0).parse().map_err(|_| "get powhash error")?,
            target: clean_0x(&self.result.2).parse().map_err(|_| "get target error")?,
            epoch: get_epoch_number(&seedhash).map_err(|()| "get epoch error")?,
            nonce: random(),
            id: 0,
        })
    }
}

// {"id":1,"jsonrpc":"2.0","result":true}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormResult {
    pub id: usize,
    pub result: bool,
    pub error: Option<Value>,
}

use crate::config::Config;
use crate::eth::pow::get_epoch_number;
use crate::state::Req;
use crate::util::clean_0x;
use bigint::H256;
use rand::random;

#[derive(Debug, Clone, Hash)]
pub struct Job {
    pub id: usize,
    pub powhash: H256,
    pub target: H256,
    pub epoch: usize,
    pub nonce: u64,
}

#[derive(Debug, Clone)]
pub struct Solution {
    pub id: usize,
    pub mixed_hash: H256,
    pub target: H256,
    pub nonce: u64,
}

pub const METHOD_LOGIN: &str = "eth_submitLogin";
pub const METHOD_GET_WORK: &str = "eth_getWork";
pub const METHOD_SUBMIT_WORK: &str = "eth_submitWork";
pub const METHOD_SUBMIT_HASHRATE: &str = "eth_submitHashrate";

// {"id":5,"method":"eth_submitWork","params":["0x43d4146cf7fe1d4e","0x2e4635265502a0f070d2d16a424f55aa797b915406de5e3685822c8d71d42e86","0x7e830f66cbd3e388920c71b92bf4d1cf429d7581854a3926841314a28530b54a"],"worker":"xox"}
pub fn make_submit(solution: &Solution, job: &Job) -> Option<Req> {
    let req = format!(
        r#"{{"id":{},"method":"{}","params":["0x{:016x}", "0x{:?}", "0x{:?}"]}}"#,
        solution.id, METHOD_SUBMIT_WORK, solution.nonce, job.powhash, solution.mixed_hash
    );
    Some((solution.id, METHOD_SUBMIT_WORK, req).into())
}

// '{"jsonrpc":"2.0", "method":"eth_submitHashrate", "params":["0x0000000000000000000000000000000000000000000000000000000000500000", "0x59daa26581d0acd1fce254fb7e85952f4c09d0915afd33d3886cd914bc7d283c"],"id":73}'
pub fn make_hashrate<N: Into<H256>>(hashrate: N) -> Req {
    let req = format!(
        r#"{{"jsonrpc":"2.0", "method":"eth_submitHashrate", "params":["0x{:?}", "0x0000000000000000000000000000000000000000000000000000000000000000"],"id":1}}"#,
        hashrate.into()
    );
    (1, METHOD_SUBMIT_HASHRATE, req).into()
}

// {"id":1,"method":"eth_submitLogin","params":["sp_yos.0v0"],"worker":"0v0"}
// {"id":2,"method":"eth_getWork","params":[]}
pub fn make_login(config: &Config) -> Req {
    let login = format!(
        r#"{{"id":1,"method":"{}","params":["{}.{}"],"worker":"{}"}}
    {{"id":1,"method":"{}","params":[]}}"#,
        METHOD_LOGIN, config.user, config.worker, config.worker, METHOD_GET_WORK
    );
    (1, METHOD_LOGIN, login).into()
}

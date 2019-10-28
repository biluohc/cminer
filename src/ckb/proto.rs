use bigint::H256;
use faster_hex::hex_string;
use futures::future::Either;
use serde_json::Value;

use crate::config::Config;
use crate::state::Req;
use crate::util::clean_0x;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodForm {
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
    pub error: Option<Value>,
}

// p: {"id":null,"method":"mining.set_target","params":["000010c6f7000000000000000000000000000000000000000000000000000000"],"error":null}
pub type FormSetTarget = (String,);

pub fn parse_target(form: FormSetTarget) -> Result<H256, &'static str> {
    clean_0x(&form.0).parse().map_err(|_| "target parse error")
}

// {"id":null,"method":"mining.notify","params":["b1404ef2","18b837ab92f44d7b0942605074c5f7e3e5244c6f07d7e939dff43d2dd87cad10",86879,"31428ebec59d5fc75c4e5f75e05130b4c9af3c85270026ab25d7ea429e606c95",true]}
pub type FormJob = (String, String, u64, String, bool);

pub fn parse_job(form: FormJob) -> Result<Job, &'static str> {
    Ok(Job {
        jobid: form.0.clone(),
        powhash: form.1.clone(),
        height: form.2,
        nonce1_bytes: 0,
        target: 0.into(),
        nonce: 0,
        id: 0,
    })
}

impl MethodForm {
    pub fn to_params(self) -> Result<Either<Job, H256>, &'static str> {
        let method = self.method.as_str();
        if method == METHOD_NOTIFY {
            serde_json::from_value(self.params)
                .map_err(|_| "deser_notify error")
                .and_then(|p: FormJob| parse_job(p).map(Either::Left))
        } else if method == METHOD_SET_TARGET {
            serde_json::from_value(self.params)
                .map_err(|_| "deser_target error")
                .and_then(|p: FormSetTarget| parse_target(p).map(Either::Right))
        } else {
            Err("unkown MethodForm")
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub struct Job {
    pub id: usize,
    pub jobid: String,
    pub powhash: String,
    pub target: H256,
    pub nonce: u128,
    pub height: u64,
    pub nonce1_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct Solution {
    pub id: usize,
    pub target: H256,
    pub nonce: u128,
}

pub const METHOD_SUBSCRIBE: &str = "mining.subscribe";
pub const METHOD_AUTHORIZE: &str = "mining.authorize";
pub const METHOD_SET_TARGET: &str = "mining.set_target";
pub const METHOD_NOTIFY: &str = "mining.notify";
pub const METHOD_SUBMIT_WORK: &str = "mining.submit";

// {"id":8,"method":"mining.submit","params":["sp_test.worker1","b1404ef2","000000000000000000084148"]}
// {"id":8,"result":true,"error":null}
pub fn make_submit(solution: &Solution, job: &Job) -> Option<Req> {
    let nonce_bytes = solution.nonce.to_be_bytes();
    let nonce_bytes_submit = &nonce_bytes[job.nonce1_bytes..];
    let nonce_submit = hex_string(nonce_bytes_submit).map_err(|e| error!("hex_string(nonce_bytes_submit) error: {:?}", e)).ok()?;

    let req = format!(r#"{{"id":{},"method":"{}","params":["{}","{}","{}"]}}"#, solution.id, METHOD_SUBMIT_WORK, "", job.jobid, nonce_submit);
    Some((solution.id, METHOD_SUBMIT_WORK, req).into())
}

// r: {"id":0,"method":"mining.subscribe","params":["ckbminer-v1.0.0",null]}
// p: {"id":0,"result":[null,"8555fd37",12],"error":null}

// r: {"id":0,"method":"mining.authorize","params":["sp_test.worker1","x"]}
// p: {"id":0,"result":true,"error":null}
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
        if let Ok((_, nonce1, nonce2)) = serde_json::from_value::<(Option<Value>, String, usize)>(self.result.clone()) {
            Ok(Either::Right((nonce1, nonce2, self.error.as_ref().map(|e| format!("{:?}", e)))))
        } else {
            Err("Invalid ResultForm")
        }
    }
}

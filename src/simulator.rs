#[macro_use]
extern crate serde;
#[macro_use]
pub extern crate nonblock_logger;

use futures::{future, SinkExt, StreamExt};
use nonblock_logger::{log::LevelFilter, BaseFilter, BaseFormater, NonblockLogger};
use parking_lot::Mutex;
use rand::random;
use serde_json;
use tokio::net::TcpListener;
use tokio::{
    codec::{FramedRead, FramedWrite, LinesCodec},
    sync::mpsc,
    timer::Interval,
};

use std::collections::HashMap as Map;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Simulator {
    job_expire: u64,
    jobs: Vec<String>,
}

pub static mut JOB: &str = "";
pub fn job() -> &'static str {
    unsafe { JOB }
}
pub fn job_set(jobs: &Vec<String>, id: &mut usize) -> &'static str {
    *id += 1;
    *id %= *id;
    unsafe { JOB = std::mem::transmute(jobs[*id].as_str()) };
    job()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let formater = BaseFormater::new().local(true).color(true).level(4);
    let filter = BaseFilter::new()
        .max_level(LevelFilter::Info)
        .starts_with(true)
        .notfound(false)
        .chain("simulator", LevelFilter::Info)
        .chain("tokio", LevelFilter::Info)
        .chain("mio", LevelFilter::Info);

    let _handle = NonblockLogger::new()
        .formater(formater)
        .filter(filter)
        .expect("add filiter failed")
        .log_to_stdout()
        .map_err(|e| eprintln!("failed to init nonblock_logger: {:?}", e))
        .unwrap();

    let confstr = include_str!("../goproxy.json");
    let confjson: serde_json::Value = serde_json::from_str(confstr)?;
    let conf: Simulator = serde_json::from_value(confjson["ckb"].clone())?;

    let serve_addr = "0.0.0.0:2510";
    let mut listener = TcpListener::bind(serve_addr).await?;
    info!("tcp listen {} ok", serve_addr);

    let miners: Arc<Mutex<Map<SocketAddr, mpsc::Sender<String>>>> = Default::default();
    let miners2 = miners.clone();
    tokio::spawn(async move {
        let mut jobid = 0;
        job_set(&conf.jobs, &mut jobid);

        let mut interval = Interval::new_interval(Duration::from_secs(conf.job_expire));
        while let Some(_) = interval.next().await {
            let job = job_set(&conf.jobs, &mut jobid);
            warn!("broadcast job {} for miners: {}", jobid, job);

            let kvs = miners2.lock().iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>();
            let dks = kvs.into_iter().filter_map(|(k, mut v)| v.try_send(job.to_owned()).err().map(|e| (e, k))).collect::<Vec<_>>();

            let dmc = {
                let mut lock = miners2.lock();
                dks.into_iter().map(|(_, k)| lock.remove(&k)).count()
            };
            warn!("broadcast job {} for miners, delete {}", jobid, dmc);
        }
    });

    loop {
        let (mut socket, addr) = match listener.accept().await {
            Ok(sa) => sa,
            Err(e) => {
                error!("tcp accept error: {:?}", e);
                continue;
            }
        };

        info!("{} accept ok", addr);

        let (mut mp, mut sc) = mpsc::channel(128);
        miners.lock().insert(addr, mp.clone());

        tokio::spawn(async move {
            let codec = LinesCodec::new_with_max_length(1024);
            let (r, w) = socket.split();

            let mut miner_r = FramedRead::new(r, codec.clone());
            let mut miner_w = FramedWrite::new(w, codec);

            let miner_r = async move {
                while let Some(req) = miner_r.next().await {
                    let req = req?;
                    info!("{}'s req: {}", addr, req);
                    mp.send(handle_ckb(req)).await?;
                }
                Ok(())
            };

            let miner_w = async move {
                while let Some(res) = sc.next().await {
                    let res = res;
                    info!("{}'s res: {}", addr, res);
                    miner_w.send(res).await?;
                }
                Ok(())
            };

            let rest: Result<_, Box<dyn Error>> = match future::select(Box::pin(miner_r), Box::pin(miner_w)).await {
                future::Either::Left((l, _)) => l,
                future::Either::Right((r, _)) => r,
            };

            warn!("{} logout with {:?}", addr, rest);
        });
    }
}

fn handle_ckb(req: String) -> String {
    let mut id = String::default();

    match serde_json::from_str::<serde_json::Value>(&req) {
        Err(e) => error!("serde_json::from_str(&req: {}) error: {:?}", req, e),
        Ok(json) => {
            let jsonid = &json["id"];
            id = serde_json::to_string(&jsonid).unwrap();

            let method = &json["method"];

            match method.as_str() {
                Some("mining.submit") => return format!(r#"{{"id":{},"jsonrpc":"2.0","result":true}}"#, id),
                Some("mining.subscribe") => return format!(r#"{{"id":{},"result":[null,"{:0>8x}",12],"error":null}}"#, id, random::<u32>()),
                Some("mining.authorize") => {
                    return format!(
                        r#"{{"id":{},"jsonrpc":"2.0","result":true}}
            {{"id":null,"method":"mining.set_target","params":["000010c6f7000000000000000000000000000000000000000000000000000000"],"error":null}}
            {}"#,
                        id,
                        job()
                    )
                }
                other => error!("invalid method({:?}): {}", other, req),
            }
        }
    }
    format!(r#"{{"id":{},"jsonrpc":"2.0","result":false, "error": "invalid request"}}"#, id)
}

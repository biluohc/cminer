#[macro_use]
extern crate serde;
#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate thiserror;
#[macro_use]
pub extern crate nonblock_logger;

use nonblock_logger::{
    chrono::Local,
    current_thread_name,
    log::{LevelFilter, Record},
    BaseFilter, BaseFormater, FixedLevel, NonblockLogger,
};

pub fn format(base: &BaseFormater, record: &Record) -> String {
    let level = FixedLevel::with_color(record.level(), base.color_get()).length(base.level_get()).into_colored().into_coloredfg();

    current_thread_name(|ctn| {
        format!(
            "[{} {}#{}:{} {}] {}\n",
            Local::now().format("%Y-%m-%d %H:%M:%S.%3f"),
            level,
            record.module_path().unwrap_or("*"),
            // record.file().unwrap_or("*"),
            record.line().unwrap_or(0),
            ctn,
            record.args()
        )
    })
}

fn main() {
    use clap::Parser;

    let config = Config::parse().fix_workers();
    let pkg = env!("CARGO_PKG_NAME");
    let log = config.log();
    println!("{}: {:?}, {:?}", pkg, log, config);

    let formater = BaseFormater::new().local(true).color(true).level(4).formater(format);
    let filter = BaseFilter::new()
        .max_level(log)
        .starts_with(true)
        .notfound(false)
        .chain(pkg, log)
        .chain("tokio", LevelFilter::Info)
        .chain("mio", LevelFilter::Info);

    let _handle = NonblockLogger::new()
        .formater(formater)
        .quiet()
        .filter(filter)
        .expect("add filiter failed")
        .log_to_stdout()
        .map_err(|e| eprintln!("failed to init nonblock_logger: {:?}", e))
        .unwrap();

    util::catch_ctrlc();

    fun(config)
}

pub mod config;
pub mod miner;
pub mod reqs;
pub mod state;
pub mod util;

pub mod btc;
pub mod ckb;
pub mod eth;
pub mod kas;

use crate::config::{Config, Currency::*};
use crate::{btc::BtcJob, ckb::CkbJob, eth::EthJob, kas::KasJob};

fn fun(config: Config) {
    match config.currency {
        Btc => miner::fun::<BtcJob>(config),
        Ckb => miner::fun::<CkbJob>(config),
        Eth => miner::fun::<EthJob>(config),
        Kas => miner::fun::<KasJob>(config),
    }
}

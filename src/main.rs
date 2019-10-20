#![feature(async_closure)]

#[macro_use]
extern crate serde;
#[macro_use]
extern crate structopt;
#[macro_use]
pub extern crate nonblock_logger;

use nonblock_logger::{log::LevelFilter, BaseFilter, BaseFormater, NonblockLogger};
use structopt::StructOpt;

fn main() {
    let config = Config::from_args().fix_workers();
    let pkg = env!("CARGO_PKG_NAME");
    let log = config.log();
    println!("{}: {:?}, {:?}", pkg, log, config);

    let formater = BaseFormater::new().local(true).color(true).level(4);
    let filter = BaseFilter::new()
        .max_level(log)
        .starts_with(true)
        .notfound(false)
        .chain(pkg, log)
        .chain("tokio", LevelFilter::Info)
        .chain("mio", LevelFilter::Info);

    let _handle = NonblockLogger::new()
        .formater(formater)
        .filter(filter)
        .expect("add filiter failed")
        .log_to_stdout()
        .map_err(|e| eprintln!("failed to init nonblock_logger: {:?}", e))
        .unwrap();

    util::catch_ctrlc();

    fun(config)
}

pub mod ckb;
pub mod config;
pub mod eth;
pub mod miner;
pub mod state;
pub mod util;

use crate::ckb::CkbJob;
use crate::config::{Config, Currency::*};
use crate::eth::EthJob;

fn fun(config: Config) {
    match config.currency {
        Eth => miner::fun::<EthJob>(config),
        Ckb => miner::fun::<CkbJob>(config),
    }
}

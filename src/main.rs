#![feature(async_closure)]

#[macro_use]
extern crate serde;
#[macro_use]
pub extern crate nonblock_logger;

use nonblock_logger::{log::LevelFilter, BaseFilter, BaseFormater, NonblockLogger};

fn main() {
    let formater = BaseFormater::new().local(true).color(true).level(4);
    let filter = BaseFilter::new().starts_with(true).chain("tokio", LevelFilter::Info).chain("mio", LevelFilter::Info);

    let _handle = NonblockLogger::new()
        .formater(formater)
        .filter(filter)
        .expect("add filiter failed")
        .log_to_stdout()
        .map_err(|e| eprintln!("failed to init nonblock_logger: {:?}", e))
        .unwrap();

    miner::fun()
}

pub mod ckb;
pub mod config;
pub mod eth;
pub mod miner;
pub mod state;
pub mod util;

#![feature(async_closure)]

#[macro_use]
pub extern crate nonblock_logger;

use nonblock_logger::{log::LevelFilter, BaseFilter, NonblockLogger};

fn main() {
    let filter = BaseFilter::new()
        .starts_with(true)
        // .notfound(false)
        .chain("tokio", LevelFilter::Info)
        .chain("mio", LevelFilter::Info);

    let _handle = NonblockLogger::new()
        .filter(filter)
        .expect("add filiter failed")
        .log_to_stdout()
        .map_err(|e| eprintln!("failed to init nonblock_logger: {:?}", e))
        .unwrap();

    client::fun()
}

pub mod ckb;
pub mod client;
pub mod config;
pub mod eth;
pub mod miner;
pub mod util;

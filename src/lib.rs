#![allow(non_upper_case_globals)]
#![allow(unused_imports)]
#[macro_use]
extern crate thiserror;
#[macro_use]
pub extern crate nonblock_logger;

use nonblock_logger::{log::LevelFilter, BaseFilter, BaseFormater, NonblockLogger};

pub fn fun<F>(fm: F)
where
    F: FnOnce(),
{
    let pkg = env!("CARGO_PKG_NAME");
    let log = LevelFilter::Info;

    let formater = BaseFormater::new().local(true).color(true).level(4);
    let filter = BaseFilter::new()
        .max_level(log)
        .starts_with(true)
        .notfound(true)
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

    fm()
}

pub mod error;

use clap::arg_enum;
use nonblock_logger::log::LevelFilter::{self, *};
use rayon::current_num_threads;

arg_enum! {
    #[derive(Debug, Clone, Copy)]
    pub enum Currency {
        Eth,
        Ckb,
    }
}

use std::{
    fmt,
    net::{SocketAddr, ToSocketAddrs},
};

#[derive(Debug, Clone, StructOpt)]
pub struct PoolAddr {
    pub str: String,
    pub sa: SocketAddr,
}

impl fmt::Display for PoolAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.str, self.sa)
    }
}

impl std::str::FromStr for PoolAddr {
    type Err = String;
    fn from_str(pool: &str) -> Result<Self, Self::Err> {
        let mut iter = pool.to_socket_addrs().map_err(|e| format!("pool.to_socket_addrs failed: {:?}", e))?;
        iter.next().map(|sa| Self { str: pool.to_owned(), sa }).ok_or("pool.to_socket_addrs is empty".into())
    }
}

#[derive(Debug, Clone, StructOpt)]
pub struct Config {
    #[structopt(short, long)]
    pub pool: PoolAddr,
    #[structopt(long, default_value = "128", help = "default is NumCPUs, if arg > it, will set as it")]
    pub workers: usize,
    #[structopt(short, long, default_value = "eth")]
    #[structopt(possible_values = &Currency::variants(), case_insensitive = true)]
    pub currency: Currency,
    #[structopt(short, long, default_value = "sp_yos")]
    pub user: String,
    #[structopt(short, long, default_value = "0v0")]
    pub worker: String,
    #[structopt(short, long, default_value = "0", parse(from_occurrences), help = "-v(Info), -vv+(Trace)")]
    pub verbose: u8,
}

impl Config {
    pub fn log(&self) -> LevelFilter {
        match self.verbose {
            0 => Warn,
            1 => Info,
            _ => Trace,
        }
    }
    pub fn new<C, P, U, W>(currency: C, pool: P, workers: usize, user: U, worker: W, verbose: u8) -> Self
    where
        C: AsRef<str>,
        P: AsRef<str>,
        U: Into<String>,
        W: Into<String>,
    {
        Self {
            workers,
            verbose,
            pool: pool.as_ref().parse().expect("resolve name failed"),
            currency: currency.as_ref().parse().unwrap_or(Currency::Eth),
            user: user.into(),
            worker: worker.into(),
        }
    }
    pub fn fix_workers(mut self) -> Self {
        let ws = current_num_threads();
        if self.workers > ws {
            self.workers = ws;
        }
        self
    }
}

pub const TIMEOUT_SECS: u64 = 3;

use std::time::Duration;
pub const fn timeout() -> Duration {
    Duration::from_secs(TIMEOUT_SECS)
}

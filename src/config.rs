#[derive(Debug, Clone)]
pub struct Config {
    pub pool: String,
    pub workers: usize,
    pub currency: String,
    pub user: String,
    pub worker: String,
}

impl Config {
    pub fn new<C, P, U, W>(currency: C, pool: P, workers: usize, user: U, worker: W) -> Self
    where
        C: Into<String>,
        P: Into<String>,
        U: Into<String>,
        W: Into<String>,
    {
        Self {
            workers,
            pool: pool.into(),
            currency: currency.into(),
            user: user.into(),
            worker: worker.into(),
        }
    }
}

use std::time::Duration;
pub const fn timeout() -> Duration {
    Duration::from_secs(3)
}

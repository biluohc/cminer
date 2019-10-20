#[derive(Debug, Clone)]
pub struct Config {
    pub workers: usize,
    pub pool: String,
}

impl Config {
    pub fn new<S: Into<String>>(workers: usize, pool: S) -> Self {
        Self {
            workers,
            pool: pool.into(),
        }
    }
}

use std::time::Duration;
pub const fn timeout() -> Duration {
    Duration::from_secs(2)
}
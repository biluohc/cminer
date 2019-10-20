use std::{error, result};

pub type Error = Box<dyn error::Error>;
pub type Result<T> = result::Result<T, Error>;

pub fn clean_0x(s: &str) -> &str {
    if s.starts_with("0x") {
        &s[2..]
    } else {
        s
    }
}

use std::sync::atomic::{AtomicUsize, Ordering};
pub fn atomic_id() -> usize {
    static AID: AtomicUsize = AtomicUsize::new(1);

    AID.fetch_add(1, Ordering::Relaxed)
}

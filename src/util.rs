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

use bigint::{H256, U256};

pub fn target_to_difficulty(target: &H256) -> U256 {
    let d = U256::from(target);
    if d <= U256::one() {
        U256::max_value()
    } else {
        ((U256::one() << 255) / d) << 1
    }
}

/// Convert an Ethash difficulty to the target. Basically just `f(x) = 2^256 / x`.
pub fn difficulty_to_target(difficulty: &U256) -> H256 {
    if *difficulty <= U256::one() {
        U256::max_value().into()
    } else {
        (((U256::one() << 255) / *difficulty) << 1).into()
    }
}

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
pub fn atomic_id() -> usize {
    static AID: AtomicUsize = AtomicUsize::new(2);

    AID.fetch_add(1, Ordering::Relaxed)
}

pub fn sleep_secs(secs: u64) {
    use std::{thread, time::Duration};
    thread::sleep(Duration::from_secs(secs))
}

pub static EXITED: AtomicBool = AtomicBool::new(false);

pub fn catch_ctrlc() {
    ctrlc::set_handler(move || {
        EXITED.store(true, Ordering::SeqCst);
        warn!("catched a ctrlc, set exited as true")
    })
    .expect("catch ctrlc error");
}

pub fn exited() -> bool {
    EXITED.load(Ordering::Relaxed)
}

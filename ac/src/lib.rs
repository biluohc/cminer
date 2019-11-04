pub trait Ac {}

use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub struct Counter(AtomicUsize, usize);

impl Counter {
    pub const fn new(init: usize) -> Self {
        Self(AtomicUsize::new(init), init)
    }
    pub fn add(&self, num: usize) -> usize {
        self.0.fetch_add(num, Ordering::Relaxed)
    }
    pub fn add_slow(&self, num: usize) -> usize {
        self.0.fetch_add(num, Ordering::SeqCst)
    }
    pub fn clear(&self) -> usize {
        self.0.swap(self.1, Ordering::SeqCst)
    }
    pub fn get(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

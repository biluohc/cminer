pub trait Ac {}

use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};

// #[derive(Debug)]
// pub struct Counter(AtomicUsize, usize);

// impl Counter {
//     pub const fn new(init: usize) -> Self {
//         Self(AtomicUsize::new(init), init)
//     }
//     pub fn add(&self, num: usize) -> usize {
//         self.0.fetch_add(num, Ordering::Relaxed)
//     }
//     pub fn add_slow(&self, num: usize) -> usize {
//         self.0.fetch_add(num, Ordering::SeqCst)
//     }
//     pub fn clear(&self) -> usize {
//         self.0.swap(self.1, Ordering::SeqCst)
//     }
//     pub fn get(&self) -> usize {
//         self.0.load(Ordering::Relaxed)
//     }
// }

// #[macro_export]
macro_rules! ac {
    ($anc: ident, $an: ident, $n: ident) => {
        #[derive(Debug)]
        pub struct $anc($an, $n);

        impl $anc {
            pub const fn new(init: $n) -> Self {
                Self(<$an>::new(init), init)
            }
            pub fn add(&self, num: $n) -> $n {
                self.0.fetch_add(num, Ordering::Relaxed)
            }
            pub fn add_slow(&self, num: $n) -> $n {
                self.0.fetch_add(num, Ordering::SeqCst)
            }
            pub fn clear(&self) -> $n {
                self.0.swap(self.1, Ordering::SeqCst)
            }
            pub fn get(&self) -> $n {
                self.0.load(Ordering::Relaxed)
            }
        }
    };
    ( $(($anc: ident, $an: ident, $n: ident)),* ) => {
        $(
            ac!($anc, $an, $n);
        )*
    };
}

// ac!(Counter, AtomicUsize, usize);
ac! {
    (AcUsize, AtomicUsize, usize),
    (AcU64, AtomicU64, u64),
    (AcU32, AtomicU32, u32),
    (AcU16, AtomicU16, u16),
    (AcU8, AtomicU8, u8)
}

#[macro_use]
extern crate ac_derive;
use ac::Ac;

#[derive(Ac, Debug)]
pub struct Metric {
    #[ac(default = 1)]
    jobs: usize,
    solutions: usize,
    #[ac(skip = true)]
    skip: u64,
}

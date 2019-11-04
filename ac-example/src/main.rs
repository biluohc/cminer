extern crate ac_example;

use ac_example::Metric;

fn main() {
    println!("Hello, proc_macro2\n\tjobs: {}\n\tsolutions: {}", Metric::jobs().get(), Metric::solutions().get());
    // println!("{}", Metric::skip().get())
}

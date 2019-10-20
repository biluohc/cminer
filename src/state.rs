use parking_lot::Mutex;
use rayon::current_num_threads;
use tokio::sync::mpsc;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use crate::config::{timeout, Config};
use crate::util;

pub type Req = (usize, &'static str, String);
pub type ReqSender = mpsc::Sender<Req>;
pub type ReqReceiver = mpsc::Receiver<Req>;

#[derive(Debug, Clone)]
pub struct Counter(Arc<AtomicUsize>, usize);

impl Counter {
    pub fn new(init: usize) -> Self {
        Self(Arc::new(AtomicUsize::new(init)), init)
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
    pub fn alives(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

#[derive(Debug)]
pub struct Worker<C> {
    pub job: State<C>,
    pub jobsc: Counter,
    pub hashrate: Counter,
    pub sender: ReqSender,
    pub idx: u64,
    pub step: u64,
}

pub trait Run: std::fmt::Debug + Send + 'static {
    fn run(&mut self);
}

#[derive(Debug, Clone)]
pub struct Statev<C> {
    pub hashrate: Counter,
    pub jobsc: Counter,
    pub job: C,
    pub submitc: usize,
    pub acceptc: usize,
    pub refusec: usize,
}

impl<C: Default> Statev<C> {
    pub fn new() -> Self {
        Self {
            hashrate: Counter::new(0),
            jobsc: Counter::new(1),
            job: C::default(),
            submitc: 0,
            acceptc: 0,
            refusec: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct State<C>(Arc<(Mutex<Statev<C>>, Config, ReqSender)>);

impl<C: Default> State<C> {
    pub fn new(mut config: Config, mp: ReqSender) -> Self {
        if config.workers > current_num_threads() {
            config.workers = current_num_threads();
        }
        Self(Arc::new((Mutex::new(Statev::new()), config, mp)))
    }
}

pub trait Handle: Clone + std::fmt::Debug {
    fn inited(&self) -> bool;
    fn login_request(&self) -> Req;
    fn handle_request(&self, req: Req) -> util::Result<String>;
    fn handle_response(&self, _resp: String) -> util::Result<()>;
}

pub trait Handler<C>: Handle {
    fn config(&self) -> &Config;
    fn value(&self) -> &Mutex<Statev<C>>;
    fn sender(&self) -> &ReqSender;
    fn login(&self) -> util::Result<()>;
    fn start_workers(&self);
}

impl<C> Handler<C> for State<C>
where
    State<C>: Handle,
    Worker<C>: Run,
{
    fn config(&self) -> &Config {
        &(self.0).1
    }
    fn value(&self) -> &Mutex<Statev<C>> {
        &(self.0).0
    }
    fn sender(&self) -> &ReqSender {
        &(self.0).2
    }
    fn login(&self) -> util::Result<()> {
        let login_request = self.login_request();
        self.sender().clone().try_send(login_request)?;
        Ok(())
    }
    fn start_workers(&self) {
        while !self.inited() {
            thread::sleep(timeout())
        }

        let n_worker = self.config().workers;
        let lock = self.value().lock();
        let lock = &*lock;

        for idx in 0..n_worker {
            let mut worker = Worker {
                job: (*self).clone(),
                jobsc: lock.jobsc.clone(),
                hashrate: lock.hashrate.clone(),
                sender: self.sender().clone(),
                idx: idx as _,
                step: n_worker as _,
            };
            rayon::spawn(move || worker.run());
        }
        info!("start {} workers", n_worker)
    }
}

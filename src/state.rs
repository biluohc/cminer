use bytesize::ByteSize;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use crate::config::{timeout, Config};
use crate::reqs::Reqs;
use crate::util::{self, DescError};

pub type ReqTuple = (usize, &'static str, String);
pub type ReqSender = mpsc::Sender<Result<Req, DescError>>;
pub type ReqReceiver = mpsc::Receiver<Result<Req, DescError>>;

#[derive(Debug, Clone)]
pub struct Req(pub usize, pub &'static str, pub String);

impl From<ReqTuple> for Req {
    fn from((i, m, q): ReqTuple) -> Self {
        Self(i, m, q)
    }
}

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

pub trait Job: Clone + Default + std::fmt::Debug + Send + 'static {
    fn jobid(&self) -> String;
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
    pub hashrates: Vec<Counter>,
    pub jobsc: Counter,
    pub job: C,
    pub reqs: Reqs,
    pub submitc: usize,
    pub acceptc: usize,
    pub rejectc: usize,
}

impl<C> Statev<C> {
    pub fn to_metric(&self) -> Metric {
        Metric {
            hashrate: self.hashrates.iter().map(|h| h.clear()).sum(),
            jobsc: self.jobsc.get(),
            submitc: self.submitc,
            acceptc: self.acceptc,
            rejectc: self.rejectc,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Metric {
    pub hashrate: usize,
    pub jobsc: usize,
    pub submitc: usize,
    pub acceptc: usize,
    pub rejectc: usize,
}

impl<C: Default> Statev<C> {
    pub fn new() -> Self {
        Self {
            reqs: Reqs::new(),
            hashrates: vec![],
            jobsc: Counter::new(1),
            job: C::default(),
            submitc: 0,
            acceptc: 0,
            rejectc: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct State<C>(Arc<(Mutex<Statev<C>>, Config, ReqSender)>);

impl<C: Default> State<C> {
    pub fn new(config: Config, mp: ReqSender) -> Self {
        Self(Arc::new((Mutex::new(Statev::new()), config, mp)))
    }
}

pub trait Handle: Clone + std::fmt::Debug + Send + Sized + 'static {
    fn login_request(&self) -> Req;
    fn hashrate_request(&self, hashrate: u64) -> Option<Req>;
    fn handle_request(&self, req: Req) -> util::Result<String>;
    fn handle_response(&self, _resp: String) -> util::Result<()>;
}

pub trait Handler<C>: Handle {
    fn config(&self) -> &Config;
    fn value(&self) -> &Mutex<Statev<C>>;
    fn sender(&self) -> &ReqSender;
    fn login(&self) -> util::Result<()>;
    fn start_workers(&self);
    fn try_show_metric(&self, secs: u64) -> bool;
    fn jobid(&self) -> Option<String>;
}

impl<C> Handler<C> for State<C>
where
    C: Job,
    Worker<C>: Run,
    State<C>: Handle,
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
        self.sender().clone().try_send(Ok(login_request))?;
        Ok(())
    }
    fn start_workers(&self) {
        let n_worker = self.config().workers;
        let mut lock = self.value().lock();
        let lock = &mut *lock;

        for idx in 0..n_worker {
            let hashrate = Counter::new(1);
            lock.hashrates.push(hashrate.clone());

            let mut worker = Worker {
                job: (*self).clone(),
                jobsc: lock.jobsc.clone(),
                sender: self.sender().clone(),
                idx: idx as _,
                step: n_worker as _,
                hashrate,
            };
            thread::spawn(move || worker.run());
        }

        info!("start {} workers", n_worker);
    }
    fn try_show_metric(&self, secs: u64) -> bool {
        self.value()
            .try_lock()
            .map(|mut lock| {
                if lock.reqs.clear_timeouts(&timeout(), |req, du| warn!("request {} timeout {:?}, {}", req.id, du, req.method)) > 0 {
                    self.sender().clone().try_send(Err("clear_timeouts".into())).expect("clear_timeouts send");
                }
                lock.to_metric()
            })
            .map(|m| {
                let secs = secs | 1;
                let hashrate = (m.hashrate as u64) / secs;

                info!(
                    "hashrate: {}, jobs: {}, submit: {}, accepted: {}, rejected: {}",
                    ByteSize(hashrate),
                    m.jobsc,
                    m.submitc,
                    m.acceptc,
                    m.rejectc
                );

                hashrate
            })
            .map(|h| {
                if let Some(req) = self.hashrate_request(h) {
                    self.sender().clone().try_send(Ok(req)).map_err(|e| error!("try send hashrate failed: {:?}", e)).ok();
                }
            })
            .is_some()
    }
    fn jobid(&self) -> Option<String> {
        self.value().try_lock().map(|l| (*l).job.jobid())
    }
}

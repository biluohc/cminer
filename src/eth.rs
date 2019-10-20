use serde_json;
use std::mem;

pub mod pow;
pub mod proto;

use crate::config::TIMEOUT_SECS;
use crate::state::{Handle, Handler, Req, Run, State, Worker};
use crate::util::{self, target_to_difficulty};

use pow::Computer;
use proto::{make_hashrate, make_login, make_submit, FormJob, Job, METHOD_SUBMIT_WORK};

#[derive(Debug, Clone)]
pub enum EthJob {
    Compute((Computer, Job)),
    Sleep,
    Exit,
}

impl EthJob {
    pub fn is_compute(&self) -> bool {
        match &self {
            Self::Compute(_) => true,
            _ => false,
        }
    }
}

impl Default for EthJob {
    fn default() -> Self {
        Self::Sleep
    }
}

impl Handle for State<EthJob> {
    fn inited(&self) -> bool {
        self.value().try_lock().map(|l| (*l).job.is_compute()).unwrap_or(false)
    }
    fn login_request(&self) -> Req {
        make_login(&self.config())
    }
    fn hashrate_request(&self, hashrate: u64) -> Option<Req> {
        Some(make_hashrate(hashrate))
    }
    fn handle_request(&self, req: Req) -> util::Result<String> {
        if req.1 == METHOD_SUBMIT_WORK {
            let mut lock = self.value().lock();
            *&mut (*lock).submitc += 1
        }
        trace!("id: {}, method: {}, req: {}", req.0, req.1, req.2);
        Ok(req.2)
    }
    fn handle_response(&self, resp: String) -> util::Result<()> {
        if let Ok(jf) = serde_json::from_str::<FormJob>(&resp) {
            match jf.to_job() {
                Ok(mut j) => {
                    info!("job: {}, epoch: {}, diff: {}, nonce: {}", j.powhash, j.epoch, target_to_difficulty(&j.target), j.nonce);
                    let mut lock = self.value().lock();
                    let lock = &mut *lock;
                    j.id = lock.jobsc.get() + 1;

                    let js = match mem::replace(&mut lock.job, EthJob::Sleep) {
                        EthJob::Compute((oc, oj)) => {
                            if j.epoch == oj.epoch {
                                EthJob::Compute((oc, j))
                            } else {
                                let c = Computer::new(j.epoch);
                                EthJob::Compute((c, j))
                            }
                        }
                        EthJob::Sleep => {
                            let c = Computer::new(j.epoch);
                            EthJob::Compute((c, j))
                        }
                        EthJob::Exit => return Ok(()),
                    };

                    lock.job = js;
                    lock.jobsc.add_slow(1);
                }
                Err(e) => error!("handle job({:?}) error: {}", jf.result, e),
            }
        } else {
            trace!("resp: {}", resp);
        }

        Ok(())
    }
}

impl Run for Worker<EthJob> {
    fn run(&mut self) {
        let mut job_idx = 0;
        let mut nonce = 0;
        let mut compute = None;

        loop {
            let job_idx2 = self.jobsc.get();
            // info!("job_idx: {}, job_idx2: {}, compute: {}", job_idx, job_idx2, compute.is_some());

            if job_idx2 != job_idx {
                let newjob = {
                    let lock = self.job.value().lock();
                    (&*lock).job.clone()
                };

                job_idx = job_idx2;
                match newjob {
                    EthJob::Compute(c) => {
                        nonce = c.1.nonce + self.idx;
                        compute = Some(c);
                    }
                    EthJob::Sleep => compute = None,
                    EthJob::Exit => break,
                }
            }

            if let Some((c, j)) = compute.as_ref() {
                if let Some(s) = c.compute(j, nonce) {
                    warn!("found a solution: id: {}, nonce: {}, pow: {}, diff: {}", s.id, nonce, j.powhash, target_to_difficulty(&s.target));
                    make_submit(&s, j).map(|req| self.sender.try_send(req).map_err(|e| error!("try send solution error: {:?}", e)).ok());
                }
                self.hashrate.add(1);
                nonce += self.step;
            } else {
                warn!("miner {} sleep {} secs", self.idx, TIMEOUT_SECS);
                util::sleep_secs(TIMEOUT_SECS);
            }
        }

        warn!("miner {} exit", self.idx);
    }
}

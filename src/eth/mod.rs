use serde_json;
use std::mem;

pub mod pow;
pub mod proto;

use crate::config::TIMEOUT_SECS;
use crate::state::{Handle, Handler, Job as JobID, Req, Run, State, Worker};
use crate::util::{self, target_to_difficulty};

use pow::Computer;
use proto::{make_hashrate, make_login, make_submit, FormJob, FormResult, Job, METHOD_SUBMIT_WORK};

#[derive(Debug, Clone)]
pub enum EthJob {
    Compute((Computer, Job)),
    Sleep,
    Exit,
}

impl JobID for EthJob {
    fn jobid(&self) -> String {
        match &self {
            Self::Compute((_, job)) => format!("{:?}", job.powhash),
            _ => "0".to_owned(),
        }
    }
}

impl Default for EthJob {
    fn default() -> Self {
        Self::Sleep
    }
}

impl Handle for State<EthJob> {
    fn login_request(&self) -> Req {
        make_login(&self.config())
    }
    fn hashrate_request(&self, hashrate: u64) -> Option<Req> {
        Some(make_hashrate(hashrate))
    }
    fn handle_request(&self, req: Req) -> util::Result<String> {
        let mut lock = self.value().lock();
        lock.reqs.add(&req);

        if req.1 == METHOD_SUBMIT_WORK {
            (*lock).submitc += 1;
        }
        trace!("id: {}, method: {}, req: {}", req.0, req.1, req.2);
        Ok(req.2)
    }
    fn handle_response(&self, resp: String) -> util::Result<()> {
        trace!("resp: {}", resp);

        if let Ok(jf) = serde_json::from_str::<FormJob>(&resp) {
            match jf.to_job() {
                Ok(mut j) => {
                    info!("job: {}, epoch: {}, diff: {}, nonce: {:0x}", j.powhash, j.epoch, target_to_difficulty(&j.target), j.nonce);
                    let mut epoch_is_old = true;
                    let mut lock = self.value().lock();
                    let lock = &mut *lock;
                    j.id = lock.jobsc.get() + 1;

                    let js = match mem::replace(&mut lock.job, EthJob::Sleep) {
                        EthJob::Compute((oc, oj)) => {
                            if j.epoch == oj.epoch {
                                EthJob::Compute((oc, j))
                            } else {
                                mem::drop(oc);
                                epoch_is_old = false;
                                lock.jobsc.add_slow(1);
                                let c = Computer::new(j.epoch, self.config().workers, self.config().testnet);
                                EthJob::Compute((c, j))
                            }
                        }
                        EthJob::Sleep => {
                            epoch_is_old = false;
                            lock.jobsc.add_slow(1);
                            let c = Computer::new(j.epoch, self.config().workers, self.config().testnet);
                            EthJob::Compute((c, j))
                        }
                        EthJob::Exit => return Ok(()),
                    };

                    lock.job = js;
                    if epoch_is_old {
                        lock.jobsc.add_slow(1);
                    }
                }
                Err(e) => error!("handle job({:?}) error: {}", jf.result, e),
            }
        } else if let Ok(FormResult { id, result, error }) = serde_json::from_str(&resp) {
            let mut lock = self.value().lock();
            let lock = &mut *lock;

            if let Some(req) = lock.reqs.remove(id) {
                let costed = req.time.elapsed();
                if req.method == METHOD_SUBMIT_WORK {
                    if result {
                        lock.acceptc += 1;
                        info!("submit {} accepted {:?}", id, costed);
                    } else {
                        lock.rejectc += 1;
                        error!("submit {} rejected {:?}, error: {:?}", id, costed, error);
                    }
                } else {
                    info!("request {}#{} {:?}, error: {:?}", id, req.method, costed, error);
                }
            } else {
                warn!("unkown response id: {}, result: {}, error: {:?}", id, result, error);
            }
        } else {
            error!("unkown resp: {}", resp);
        }

        Ok(())
    }
}

impl Run for Worker<EthJob> {
    fn run(&mut self) {
        let mut job_idx = 0;
        let mut nonce = 0.into();
        let mut compute = None;

        loop {
            let job_idx2 = self.jobsc.get();
            // info!("job_idx: {}, job_idx2: {}, compute: {}", job_idx, job_idx2, compute.is_some());

            if job_idx2 != job_idx {
                compute.take();
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
                if let Some(s) = c.compute(j, &nonce) {
                    warn!("found a solution: id: {}, nonce: {:0x}, powhash: {}, diff: {}", s.id, nonce, j.powhash, target_to_difficulty(&s.target));
                    make_submit(&s, j).map(|req| self.sender.try_send(Ok(req)).map_err(|e| error!("try send solution error: {:?}", e)).ok());
                }
                self.hashrate.add(1);
                nonce = nonce + self.step;
            } else {
                trace!("miner {} will sleep {} secs", self.idx, TIMEOUT_SECS);
                util::sleep_secs(TIMEOUT_SECS);
            }
        }

        warn!("miner {} exit", self.idx);
    }
}

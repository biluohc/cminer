use bigint::H256;
use futures::future::Either;
use serde_json;
use std::mem;

pub mod pow;
pub mod proto;

use crate::config::TIMEOUT_SECS;
use crate::state::{Handle, Handler, Req, Run, State, Worker};
use crate::util::{self, target_to_difficulty};

use pow::{parse_nonce, Computer};
use proto::{make_login, make_submit, Job, MethodForm, ResultForm, METHOD_SUBMIT_WORK};

#[derive(Debug, Clone)]
pub enum CkbJob {
    Nonce1t((u128, usize, H256)),
    Compute(Job),
    Sleep,
    Exit,
}

impl CkbJob {
    pub fn is_compute(&self) -> bool {
        match &self {
            Self::Compute(_) => true,
            _ => false,
        }
    }
}

impl Default for CkbJob {
    fn default() -> Self {
        Self::Sleep
    }
}

impl Handle for State<CkbJob> {
    fn inited(&self) -> bool {
        self.value().try_lock().map(|l| (*l).job.is_compute()).unwrap_or(false)
    }
    fn login_request(&self) -> Req {
        make_login(&self.config())
    }
    fn hashrate_request(&self, _: u64) -> Option<Req> {
        None
    }
    fn handle_request(&self, req: Req) -> util::Result<String> {
        let mut lock = self.value().lock();
        lock.reqs.add(&req);
        if req.1 == METHOD_SUBMIT_WORK {
            *&mut (*lock).submitc += 1;
        }
        trace!("id: {}, method: {}, req: {}", req.0, req.1, req.2);
        Ok(req.2)
    }
    fn handle_response(&self, resp: String) -> util::Result<()> {
        trace!("resp: {}", resp);

        if let Ok(jf) = serde_json::from_str::<MethodForm>(&resp) {
            match jf.to_params() {
                Ok(jt) => {
                    let mut lock = self.value().lock();
                    let lock = &mut *lock;

                    match jt {
                        Either::Left(mut j) => {
                            j.id = lock.jobsc.get() + 1;
                            let (nonce, nonce1_bytes, target) = match mem::replace(&mut lock.job, CkbJob::Sleep) {
                                CkbJob::Compute(oj) => (oj.nonce, oj.nonce1_bytes, oj.target),
                                CkbJob::Nonce1t(n1t) => n1t,
                                CkbJob::Sleep => {
                                    fatal!("job arrived, but nonce1 info is none");
                                }
                                CkbJob::Exit => return Ok(()),
                            };

                            j.target = target;
                            j.nonce = nonce;
                            j.nonce1_bytes = nonce1_bytes;

                            info!(
                                "job: {}, height: {}, powhash: {}, diff: {}, nonce: {:0x}",
                                j.jobid,
                                j.height,
                                j.powhash,
                                target_to_difficulty(&j.target),
                                j.nonce
                            );
                            let js = CkbJob::Compute(j);
                            lock.job = js;
                            lock.jobsc.add_slow(1);
                        }
                        Either::Right(target) => {
                            let job = match mem::replace(&mut lock.job, CkbJob::Sleep) {
                                CkbJob::Sleep => CkbJob::Nonce1t((0, 0, target)),
                                CkbJob::Nonce1t((n1, n1b, _)) => CkbJob::Nonce1t((n1, n1b, target)),
                                CkbJob::Compute(mut job) => {
                                    job.target = target;
                                    CkbJob::Compute(job)
                                }
                                other => other,
                            };
                            lock.job = job;
                        }
                    }
                }
                Err(e) => error!("handle job({}) error: {}", resp, e),
            }
        } else if let Ok(rf) = serde_json::from_str::<ResultForm>(&resp) {
            // <(id, bool, _), (nonce1, nonce2, _)>
            match rf.to_result() {
                Ok(Either::Left((id, b, e))) => {
                    let mut lock = self.value().lock();
                    let lock = &mut *lock;

                    if let Some(req) = lock.reqs.remove(id) {
                        let costed = req.time.elapsed();
                        if req.method == METHOD_SUBMIT_WORK {
                            if b {
                                lock.acceptc += 1;
                                info!("submit {} accepted {:?}", req.id, costed);
                            } else {
                                lock.rejectc += 1;
                                error!("submit {} rejected {:?}, error: {:?}", req.id, costed, e);
                            }
                        } else {
                            info!("request {}#{} {:?}, error: {:?}", req.id, req.method, costed, e);
                        }
                    } else {
                        warn!("unkown response id: {}, result: {}, error: {:?}", id, b, e);
                    }
                }
                Ok(Either::Right((nonce1, nonce2, e))) => {
                    info!("nonce1: {}, nonce2_bytes: {}, error: {:?}", nonce1, nonce2, e);
                    let (n1, n1b) = parse_nonce(&nonce1);

                    let mut lock = self.value().lock();
                    let lock = &mut *lock;
                    let job = match mem::replace(&mut lock.job, CkbJob::Sleep) {
                        CkbJob::Sleep => CkbJob::Nonce1t((n1, n1b, 0.into())),
                        CkbJob::Nonce1t((_n1, _n1b, t)) => CkbJob::Nonce1t((n1, n1b, t)),
                        other => other,
                    };
                    lock.job = job;
                }
                Err(e) => error!("handle result({}) error:  {}", resp, e),
            }
        } else {
            error!("unkown resp: {}", resp);
        }

        Ok(())
    }
}

impl Run for Worker<CkbJob> {
    fn run(&mut self) {
        let mut job_idx = 0;
        let mut job = None;
        let mut nonce = 0u128;
        let mut computer = Computer::new();

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
                    CkbJob::Compute(j) => {
                        nonce = j.nonce + self.idx as u128;
                        computer.update(&j.powhash);
                        job = Some(j);
                    }
                    CkbJob::Sleep => job = None,
                    CkbJob::Nonce1t(..) => job = None,
                    CkbJob::Exit => break,
                }
            }

            if let Some(j) = job.as_ref() {
                if let Some(s) = computer.compute(j, nonce) {
                    warn!("found a solution: id: {}, nonce: {:0x}, jobid: {}, diff: {}", s.id, nonce, j.jobid, target_to_difficulty(&s.target));
                    make_submit(&s, j).map(|req| self.sender.try_send(req).map_err(|e| error!("try send solution error: {:?}", e)).ok());
                }
                self.hashrate.add(1);
                nonce += self.step as u128;
            } else {
                warn!("miner {} will sleep {} secs", self.idx, TIMEOUT_SECS);
                util::sleep_secs(TIMEOUT_SECS);
            }
        }

        warn!("miner {} exit", self.idx);
    }
}

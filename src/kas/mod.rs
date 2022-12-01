use kaspow::{target2difficulty, Uint256};
use serde_json;
use std::mem;

pub mod pow;
pub mod proto;

use crate::config::TIMEOUT_SECS;
use crate::state::{Handle, Handler, Job as JobID, Req, Run, State, Worker};
use crate::util;

use pow::Computer;
use proto::{make_hashrate, make_login, make_submit, Job, MethodForm, MethodParams, ResultForm, METHOD_SUBMIT_WORK};

#[derive(Debug, Clone)]
pub enum KasJob {
    // (nonce, nonce1_bytes, target)
    Nonce1t((u64, usize, Uint256)),
    Compute(Job),
    Sleep,
    Exit,
}

impl JobID for KasJob {
    fn jobid(&self) -> String {
        match &self {
            Self::Compute(job) => job.jobid.clone(),
            _ => "0".to_owned(),
        }
    }
}

impl Default for KasJob {
    fn default() -> Self {
        Self::Sleep
    }
}

impl Handle for State<KasJob> {
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

        if let Ok(jf) = serde_json::from_str::<MethodForm>(&resp) {
            match jf.to_params() {
                Ok(jt) => {
                    let mut lock = self.value().lock();
                    let lock = &mut *lock;

                    match jt {
                        MethodParams::Job(mut j) => {
                            j.id = lock.jobsc.get() + 1;
                            let (nonce, nonce1_bytes, target) = match mem::replace(&mut lock.job, KasJob::Sleep) {
                                KasJob::Compute(oj) => (oj.nonce, oj.nonce1_bytes, oj.target),
                                KasJob::Nonce1t(n1t) => n1t,
                                KasJob::Sleep => {
                                    fatal!("job arrived, but nonce1 info is none");
                                }
                                KasJob::Exit => return Ok(()),
                            };

                            j.target = target;
                            j.nonce1_bytes = nonce1_bytes;
                            if j.nonce1_bytes == 0 {
                                j.nonce = nonce + rand::random::<u64>() / 2;
                            }

                            let diff = target2difficulty(&j.target);
                            info!("job: {}, timestamp: {}, powhash: {}, diff: {}, nonce: {:0x}", j.jobid, j.timestamp, j.powhash, diff, j.nonce);

                            if diff < 1u64 {
                                fatal!("don't received set_difficulty");
                            }

                            let js = KasJob::Compute(j);
                            lock.job = js;
                            lock.jobsc.add_slow(1);
                        }
                        MethodParams::Target(target) => {
                            let job = match mem::replace(&mut lock.job, KasJob::Sleep) {
                                KasJob::Sleep => KasJob::Nonce1t((0, 0, target)),
                                KasJob::Nonce1t((n1, n1b, _)) => KasJob::Nonce1t((n1, n1b, target)),
                                KasJob::Compute(mut job) => {
                                    job.target = target;
                                    KasJob::Compute(job)
                                }
                                other => other,
                            };
                            lock.job = job;
                        }
                        MethodParams::Nonce1t((n1, n1b)) => {
                            let job = match mem::replace(&mut lock.job, KasJob::Sleep) {
                                KasJob::Sleep => KasJob::Nonce1t((0, 0, 0.into())),
                                KasJob::Nonce1t((_n1, _n1b, t)) => KasJob::Nonce1t((n1, n1b, t)),
                                KasJob::Compute(mut job) => {
                                    job.nonce = n1;
                                    job.nonce1_bytes = n1b;
                                    KasJob::Compute(job)
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
            // <(id, bool, _)
            match rf.to_result() {
                Ok((id, b, e)) => {
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
                Err(e) => error!("handle result({}) error:  {}", resp, e),
            }
        } else {
            error!("unkown resp: {}", resp);
        }

        Ok(())
    }
}

impl Run for Worker<KasJob> {
    fn run(&mut self) {
        let mut job_idx = 0;
        let mut job = None;
        let mut nonce = 0u64;
        let mut computer = Computer::new(self.testnet);

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
                    KasJob::Compute(j) => {
                        nonce = j.nonce + self.idx;
                        // computer.update(&j.powhash);
                        job = Some(j);
                    }
                    KasJob::Sleep => job = None,
                    KasJob::Nonce1t(..) => job = None,
                    KasJob::Exit => break,
                }
            }

            if let Some(j) = job.as_ref() {
                if let Some(s) = computer.compute(j, nonce) {
                    warn!("found a solution: id: {}, nonce: {:0x}, jobid: {}, diff: {}", s.id, nonce, j.jobid, target2difficulty(&s.target));
                    make_submit(&s, j).map(|req| self.sender.try_send(Ok(req)).map_err(|e| error!("try send solution error: {:?}", e)).ok());
                }
                self.hashrate.add(1);
                nonce += self.step;
            } else {
                trace!("miner {} will sleep {} secs", self.idx, TIMEOUT_SECS);
                util::sleep_secs(TIMEOUT_SECS);
            }
        }

        warn!("miner {} exit", self.idx);
    }
}

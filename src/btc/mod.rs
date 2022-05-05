use bitcoin::util::uint::Uint256;
use futures::future::Either;
use serde_json;
use std::mem;

pub mod pow;
pub mod proto;

use crate::config::TIMEOUT_SECS;
use crate::state::{Handle, Handler, Job as JobID, Req, Run, State, Worker};
use crate::util;

use pow::{target_to_difficulty, unit_target, Computer};
use proto::{make_login, make_submit, Job, MethodForm, ResultForm, METHOD_SUBMIT_WORK};

#[derive(Debug, Clone)]
pub enum BtcJob {
    Nonce1t((String, usize, u128, Uint256)),
    Compute(Job),
    Sleep,
    Exit,
}

impl JobID for BtcJob {
    fn jobid(&self) -> String {
        match &self {
            Self::Compute(job) => job.jobid.clone(),
            _ => "0".to_owned(),
        }
    }
}

impl Default for BtcJob {
    fn default() -> Self {
        Self::Sleep
    }
}

impl Handle for State<BtcJob> {
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
                        Either::Left(mut j) => {
                            j.id = lock.jobsc.get() + 1;
                            let (nonce1, nonce2_bytes, nonce2_max, target) = match mem::replace(&mut lock.job, BtcJob::Sleep) {
                                BtcJob::Compute(oj) => (oj.nonce1, oj.nonce2_bytes, oj.nonce2_max, oj.target),
                                BtcJob::Nonce1t(n1t) => n1t,
                                BtcJob::Sleep => {
                                    fatal!("job arrived, but nonce1 info is none");
                                }
                                BtcJob::Exit => return Ok(()),
                            };

                            use rand::{thread_rng, Rng};
                            j.nonce2 = thread_rng().gen_range(0, nonce2_max / 2);

                            j.nonce1 = nonce1;
                            j.target = target;
                            j.nonce2_max = nonce2_max;
                            j.nonce2_bytes = nonce2_bytes;

                            info!(
                                "job: {}, diff {}: {}, prevhash: {}, nonce1: {}, nonce2: {:x}, nbits: {}, ntime: {}, version: {}",
                                j.jobid,
                                target_to_difficulty(&j.target),
                                j.target,
                                j.prev_hash,
                                j.nonce1,
                                j.nonce2,
                                j.nbits,
                                j.ntime,
                                j.version,
                            );
                            let js = BtcJob::Compute(j);
                            lock.job = js;
                            lock.jobsc.add_slow(1);
                        }
                        Either::Right(diff) => {
                            let target = unit_target() / Uint256::from_u64(diff).unwrap();
                            let job = match mem::replace(&mut lock.job, BtcJob::Sleep) {
                                BtcJob::Sleep => BtcJob::Nonce1t(("".to_owned(), 0, 0, target)),
                                BtcJob::Nonce1t((n1, n2b, n2m, _)) => BtcJob::Nonce1t((n1, n2b, n2m, target)),
                                BtcJob::Compute(mut job) => {
                                    job.target = target;
                                    BtcJob::Compute(job)
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
                Ok(Either::Right((nonce1, nonce2_bytes, e))) => {
                    let nonce2_max = 2u128.pow(8 * nonce2_bytes as u32);
                    info!("nonce1: {}, nonce2_bytes: {}, nonce2_max: {:x}, error: {:?}", nonce1, nonce2_bytes, nonce2_max, e);

                    let mut lock = self.value().lock();
                    let lock = &mut *lock;
                    let job = match mem::replace(&mut lock.job, BtcJob::Sleep) {
                        BtcJob::Sleep => BtcJob::Nonce1t((nonce1, nonce2_bytes, nonce2_max, Default::default())),
                        BtcJob::Nonce1t((_, _, _, t)) => BtcJob::Nonce1t((nonce1, nonce2_bytes, nonce2_max, t)),
                        BtcJob::Compute(mut j) => {
                            j.nonce1 = nonce1;
                            j.nonce2_bytes = nonce2_bytes;
                            j.nonce2_max = nonce2_max;
                            BtcJob::Compute(j)
                        }
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

impl Run for Worker<BtcJob> {
    fn run(&mut self) {
        let mut job_idx = 0;
        let mut job = None;
        let mut nonce = 0u32;
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
                    BtcJob::Compute(mut j) => {
                        j.nonce2 += self.idx as u128;
                        computer.update(&j);
                        job = Some(j);
                        nonce = 0;
                    }
                    BtcJob::Sleep => job = None,
                    BtcJob::Nonce1t(..) => job = None,
                    BtcJob::Exit => break,
                }
            }

            if let Some(j) = job.as_mut() {
                if let Some(solution) = computer.compute(&*j, nonce) {
                    warn!(
                        "found a solution: id: {}, nonce1&2: {} {:x}, nonce: {:0x}, jobid: {}, diff: {}, target: {}",
                        solution.id,
                        j.nonce1,
                        j.nonce2,
                        solution.nonce,
                        j.jobid,
                        target_to_difficulty(&solution.target),
                        solution.target,
                    );
                    make_submit(&solution, j).map(|req| self.sender.try_send(Ok(req)).map_err(|e| error!("try send solution error: {:?}", e)).ok());
                    util::sleep_secs(self.sleep);
                }
                self.hashrate.add(1);
                nonce += 1;

                if nonce == 0 {
                    let nonce2 = j.nonce2;
                    j.nonce2 += self.step as u128;
                    info!("worker-{} nonce2 update: {} + {} -> {}", self.idx, nonce2, self.step, j.nonce2);
                    computer.update(j);
                }
            } else {
                trace!("miner {} will sleep {} secs", self.idx, TIMEOUT_SECS);
                util::sleep_secs(TIMEOUT_SECS);
            }
        }

        warn!("miner {} exit", self.idx);
    }
}

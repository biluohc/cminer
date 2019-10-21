use std::{
    collections::BTreeMap as Map,
    time::{Duration, Instant},
};

use crate::state::Req as RawReq;

#[derive(Debug, Clone)]
pub struct Req {
    pub id: usize,
    pub method: &'static str,
    pub time: Instant,
}

impl From<&RawReq> for Req {
    fn from(raw: &RawReq) -> Self {
        Self {
            id: raw.0,
            method: raw.1,
            time: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Reqs {
    data: Map<usize, Req>,
}

impl Reqs {
    pub fn new() -> Self {
        Self { data: Map::new() }
    }
    pub fn add<R>(&mut self, req: R) -> Option<Req>
    where
        R: Into<Req>,
    {
        let req = req.into();
        self.data.insert(req.id, req)
    }
    pub fn remove(&mut self, id: usize) -> Option<Req> {
        self.data.remove(&id)
    }
    pub fn clear_timeouts<F>(&mut self, timeout: &Duration, f: F) -> usize
    where
        F: Fn(Req, Duration),
    {
        let kds = self
            .data
            .values()
            .filter_map(|req| {
                let d = req.time.elapsed();
                if d >= *timeout {
                    Some((req.id, d))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        kds.into_iter().map(|(k, d)| self.remove(k).map(|req| f(req, d))).count()
    }
}

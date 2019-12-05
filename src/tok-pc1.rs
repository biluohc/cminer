#![allow(unreachable_code)]
#[macro_use]
pub extern crate nonblock_logger;
extern crate toktt;

fn main() {
    toktt::fun(fun)
}

use parking_lot::Mutex;
use std::{
    collections::BTreeMap,
    ops::DerefMut,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

fn fun() {
    let tn = format!("tokl");
    let (mut mp, sc) = sync::mpsc::unbounded_channel();

    let results = Arc::new(Mutex::default());
    let results2 = results.clone();

    thread::Builder::new()
        .name(tn)
        .spawn(move || {
            let mut runtime = CurrentThreadRuntime::new().unwrap();

            let server = sc
                .map_err(Error::from)
                .and_then(|x| x)
                .for_each(move |(ch, idx)| {
                    handle_chan(ch, idx, results2.clone());
                    AC_CHAN.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .then(|rest| Ok::<(), ()>(info!("tok finish: {:?}", rest)));

            runtime.block_on(server).unwrap();

            warn!("tok finish");
        })
        .unwrap();

    let mut map = std::collections::BTreeMap::new();
    for idx in 0..10000 {
        let (mp3, sc3) = sync::mpsc::channel(8);
        mp.try_send(Ok((sc3, idx))).unwrap();
        map.insert(idx, mp3);
    }

    thread::sleep(Duration::from_secs(10));

    for (idx, mp3) in map.iter_mut() {
        mp3.try_send(*idx).unwrap();
    }

    thread::sleep(Duration::from_secs(10));
    mp.try_send(Err(DescError::from("10 secs").into())).unwrap();
    // drop(mp);
    // handle.join().unwrap();

    let res = results.lock().deref_mut().clone();

    info!(
        "map: {}, res: {}, ac_chan: {}, ac_req: {}",
        map.len(),
        res.len(),
        AC_CHAN.load(Ordering::SeqCst),
        AC_REQ.load(Ordering::SeqCst)
    );
}

static AC_CHAN: AtomicUsize = AtomicUsize::new(0);
static AC_REQ: AtomicUsize = AtomicUsize::new(0);

use futures1::{Future, Stream};
use tokio1::runtime::current_thread::{spawn, Runtime as CurrentThreadRuntime};
use tokio1::sync;
use toktt::error::{DescError, Error};

// copy can also cause errors..
fn handle_chan(
    sc: sync::mpsc::Receiver<usize>,
    idx: usize,
    mp: Arc<Mutex<BTreeMap<usize, Instant>>>,
) {
    let copy = sc
        .map_err(Error::from)
        .for_each(move |msg| {
            // info!("handle {} await res: {}", idx, msg);
            assert_eq!(msg, idx);
            AC_REQ.fetch_add(1, Ordering::SeqCst);
            let mut lock = mp.lock();
            assert!(lock.deref_mut().insert(msg, Instant::now()).is_none());
            Ok(())
        })
        .then(move |rest| {
            if let Err(e) = rest {
                warn!("loop_req {} failed: {:?}", idx, e);
            }
            Ok(())
        });

    spawn(copy);
}

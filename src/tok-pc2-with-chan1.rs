#![allow(dead_code)]
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
    let (mp, mut sc) = sync::mpsc::unbounded_channel();

    let results = Arc::new(Mutex::default());
    let results2 = results.clone();

    thread::Builder::new()
        .name(tn)
        .spawn(move || {
            let mut rt = runtime::Builder::new()
                .basic_scheduler()
                .enable_all()
                .build()
                .unwrap();

            let local = task::LocalSet::new();
            local.block_on(&mut rt, async move {
                while let Some(ce) = sc.recv().await {
                    if let Ok((c, idx)) = ce {
                        // info!("handle {}", idx);
                        AC_CHAN.fetch_add(1, Ordering::SeqCst);
                        hanlde_chan(c, idx, results2.clone())
                    } else {
                        break;
                    }
                }
            });

            info!("tok finish");
        })
        .unwrap();

    let mut map = std::collections::BTreeMap::new();
    for idx in 0..10000 {
        let (mp3, sc3) = tokio1::sync::mpsc::channel(8);
        mp.send(Ok((sc3, idx))).unwrap();
        map.insert(idx, mp3);
    }

    thread::sleep(Duration::from_secs(10));

    for (idx, mp3) in map.iter_mut() {
        mp3.try_send(*idx).unwrap();
    }

    thread::sleep(Duration::from_secs(10));
    mp.send(Err(())).unwrap();
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
use futures::{compat, TryStreamExt};
use tokio::{runtime, sync, task};

// copy can also cause errors..
fn hanlde_chan(
    sc: tokio1::sync::mpsc::Receiver<usize>,
    idx: usize,
    mp: Arc<Mutex<BTreeMap<usize, Instant>>>,
) {
    let mut sc = compat::Compat01As03::new(sc);
    let copy = async move {
        while let Ok(Some(msg)) = sc.try_next().await {
            // info!("handle {} await res: {}", idx, msg);
            assert_eq!(msg, idx);
            AC_REQ.fetch_add(1, Ordering::SeqCst);
            let mut lock = mp.lock();
            assert!(lock.deref_mut().insert(msg, Instant::now()).is_none());
        }
    };
    task::spawn_local(copy);
}

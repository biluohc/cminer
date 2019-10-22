// #![warn(rust_2018_idioms)]
use futures::{future, SinkExt};
use tokio::{
    codec::{FramedRead, FramedWrite, LinesCodec},
    net::TcpStream,
    prelude::*,
    runtime::current_thread::Runtime,
    sync::mpsc,
};

use std::error::Error;
use std::{thread, time::Instant};

use crate::{
    config::{timeout, Config},
    state::{Handler, ReqReceiver, State},
    util::{exited, sleep_secs, DescError},
};

pub fn fun<C>(config: Config)
where
    C: Default,
    State<C>: Handler<C>,
{
    let (mp, mut sc) = mpsc::channel(32);
    let state: State<C> = State::new(config, mp);

    let state_clone = state.clone();
    let _client = thread::Builder::new()
        .name("toko".into())
        .spawn(move || {
            let mut runtime = Runtime::new().expect("client Runtime new failed");

            let mut count = 0;
            loop {
                let start_time = Instant::now();
                runtime.block_on(connect(&state_clone, &mut sc, count, &start_time).then(|e| {
                    error!("#{} connect finish {:?} of {:?}, will sleep 5 secs\n", count, start_time.elapsed(), e);
                    future::ready(())
                }));

                sleep_secs(5);
                count += 1;
            }
        })
        .unwrap();

    state.start_workers();

    let mut now = Instant::now();
    let mut jobnow = Instant::now();
    let mut jobid = "".to_owned();
    let expire = state.config().expire;
    while !exited() {
        let secs = now.elapsed().as_secs();
        if secs >= 30 && state.try_show_metric(secs) {
            now = Instant::now();
        }
        if jobnow.elapsed().as_secs() >= expire {
            let jobid2 = state.jobid();
            if let Some(id2) = jobid2 {
                if jobid == id2 {
                    warn!("job {} alives > {} secs, expired", jobid, expire);
                    state.sender().clone().try_send(Err("job expired".into())).expect("job expired send");
                } else {
                    jobid = id2;
                }
            }
            jobnow = Instant::now();
        }
        sleep_secs(1);
    }

    state.try_show_metric(now.elapsed().as_secs());
}

async fn connect<C, S>(state: &S, sc: &mut ReqReceiver, count: usize, start_time: &Instant) -> Result<(), Box<dyn Error>>
where
    S: Handler<C>,
{
    let mut stream = TcpStream::connect(&state.config().pool.sa).timeout(timeout()).await??;

    info!("#{} tcp connect to {} ok", count, state.config().pool);

    let codec = LinesCodec::new_with_max_length(1024);
    let (r, w) = stream.split();

    let miner_r = FramedRead::new(r, codec.clone()).for_each(|resp| {
        if let Err(e) = resp.map_err(|e| Box::new(e) as _).and_then(|resp| state.handle_response(resp)) {
            error!("resp error: {:?}", e);
        }
        future::ready(())
    });

    let mut miner_w = FramedWrite::new(w, codec);

    // send login request
    let req = state.handle_request(state.login_request()).expect("handle_request(login_request)");
    miner_w.send(req).timeout(timeout()).await??;

    let miner_w = loop_handle_request(sc, miner_w, state, start_time);

    match future::select(Box::pin(miner_w), miner_r).await {
        future::Either::Left((l, _)) => info!("#{} select finish left(w): {:?}", count, l?),
        future::Either::Right((r, _)) => info!("#{} select finish righ(r): {:?}", count, r),
    }

    Ok(())
}

async fn loop_handle_request<C, S, W>(sc: &mut ReqReceiver, mut miner_w: W, state: &S, start_time: &Instant) -> Result<(), Box<dyn Error>>
where
    S: Handler<C>,
    W: SinkExt<String> + std::marker::Unpin,
{
    while let Some(msg) = sc.next().await {
        let req = match msg {
            Ok(req) => req,
            Err(e) => {
                let error_time: &Instant = e.as_ref();
                if error_time <= start_time {
                    warn!("skip error message belongs to the last connection: {}", e);
                    continue;
                } else {
                    return Err(e.into());
                }
            }
        };
        let req = state.handle_request(req)?;
        let ok = miner_w.send(req).timeout(timeout()).await?;
        if ok.is_err() {
            return Err(DescError::from("miner_w.send(msg).timeout()").into());
        }
    }

    Ok(())
}

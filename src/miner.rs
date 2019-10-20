// #![warn(rust_2018_idioms)]
use futures::{future, SinkExt};
use tokio::{
    codec::{FramedRead, FramedWrite, LinesCodec},
    net::TcpStream,
    prelude::*,
    runtime::current_thread::Runtime,
};

use std::error::Error;
use std::{thread, time};

use crate::{
    config::timeout,
    state::{Handler, ReqReceiver, State},
    util::{exited, sleep_secs},
};

pub fn fun<C>(state: State<C>, mut sc: ReqReceiver)
where
    State<C>: Handler<C>,
{
    state.login().unwrap();

    let state_clone = state.clone();
    let _client = thread::Builder::new()
        .name("toko".into())
        .spawn(move || {
            let mut runtime = Runtime::new().expect("client Runtime new failed");

            let mut count = 0;
            loop {
                runtime.block_on(connect(&state_clone, &mut sc, count).then(|e| {
                    error!("#{} connect finish: {:?}, will sleep 5 secs\n", count, e);
                    future::ready(())
                }));

                sleep_secs(5);
                count += 1;
            }
        })
        .unwrap();

    while !state.try_start_workers() {
        if exited() {
            return;
        }
        sleep_secs(1);
    }

    let mut now = time::Instant::now();
    while !exited() {
        let secs = now.elapsed().as_secs();
        if secs >= 30 {
            if state.try_show_metric(secs) {
                now = time::Instant::now();
            }
        }
        sleep_secs(1);
    }

    state.try_show_metric(now.elapsed().as_secs());
}

async fn connect<C, S>(state: &S, sc: &mut ReqReceiver, count: usize) -> Result<(), Box<dyn Error>>
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

    let miner_w = FramedWrite::new(w, codec);
    let miner_w = sc.fold(Ok(miner_w), async move |mw: Result<_, Box<dyn Error>>, msg| match state.handle_request(msg) {
        Ok(msg) => match mw {
            Ok(mut miner_w) => match miner_w.send(msg).timeout(timeout()).await {
                Ok(Ok(())) => Ok(miner_w),
                Ok(Err(e)) => Err(Box::new(e) as _),
                Err(e) => Err(Box::new(e) as _),
            },
            Err(e) => fatal!("unreachable#miner_w.fold Err2: {:?}", e),
        },
        Err(e) => Err(e),
    });

    match future::select(Box::pin(miner_w), miner_r).await {
        future::Either::Left((l, _)) => info!("#{} select finish left(w): {:?}", count, l?),
        future::Either::Right((r, _)) => info!("#{} select finish righ(r): {:?}", count, r),
    }

    Ok(())
}

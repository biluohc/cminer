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
use std::{thread, time};

use crate::{
    config::{timeout, Config},
    eth::EthJob,
    state::{Handler, ReqReceiver, State},
};

pub fn fun() {
    let config = Config::new("eth", "39.106.195.31:3711", 32, "sp_yos", "0v0");

    let (mp, mut sc) = mpsc::channel(32);
    let state: State<EthJob> = State::new(config, mp.clone());
    state.login().unwrap();

    let state_clone = state.clone();
    let client = thread::Builder::new()
        .name("toko".into())
        .spawn(move || {
            let mut runtime = Runtime::new().expect("client Runtime new failed");

            let mut count = 0;
            loop {
                runtime.block_on(connect(&state_clone, &mut sc, count).then(|e| {
                    error!("#{} connect finish: {:?}, will sleep 5 secs\n", count, e);
                    future::ready(())
                }));

                thread::sleep(time::Duration::from_secs(5));
                count += 1;
            }
        })
        .unwrap();

    state.start_workers();

    client.join().expect("client thread join failed")
}

async fn connect<C, S>(state: &S, sc: &mut ReqReceiver, count: usize) -> Result<(), Box<dyn Error>>
where
    S: Handler<C>,
{
    let mut stream = TcpStream::connect(&state.config().pool).timeout(timeout()).await??;

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

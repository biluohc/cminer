#![warn(rust_2018_idioms)]

use std::error::Error;
use std::{thread, time};

use futures::{future, SinkExt};
use tokio::{
    codec::{FramedRead, FramedWrite, LinesCodec},
    net::TcpStream,
    prelude::*,
    runtime::current_thread::Runtime,
    sync::mpsc,
};

use crate::config::{timeout, Config};

pub fn fun() {
    let config = Config::new(2, "39.106.195.31:3711");

    let (mut mp, mut sc) = mpsc::channel(32);

    let auth = r#"{"id":1,"method":"eth_submitLogin","params":["sp_yos"],"worker":"xox"}
    {"id":2,"method":"eth_getWork","params":[]}"#;
    mp.try_send(auth.into()).unwrap();

    let client = thread::Builder::new()
        .name("toko".into())
        .spawn(move || {
            let mut runtime = Runtime::new().expect("client Runtime new failed");

            let mut count = 0;
            loop {
                runtime.block_on(connect(&config, &mut sc, count).then(|e| {
                    error!("#{} connect finish: {:?}, will sleep 5 secs\n", count, e);
                    future::ready(())
                }));

                thread::sleep(time::Duration::from_secs(5));
                count += 1;
            }
        })
        .unwrap();

    thread::sleep(timeout() * 5);
    mp.try_send(r#"{"id":22,"method":"eth_getWork","params":[]}"#.into())
        .map(|()| info!("send getwork22 ok"))
        .unwrap();

    client.join().expect("client thread join failed")
}

async fn connect(config: &Config, sc: &mut mpsc::Receiver<String>, count: usize) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect(&config.pool).timeout(timeout()).await??;

    info!("#{} tcp connect to {} ok", count, config.pool);

    let codec = LinesCodec::new_with_max_length(1024);
    let (r, w) = stream.split();

    let miner_r = FramedRead::new(r, codec.clone()).for_each(|resp| {
        match resp {
            Ok(s) => info!("resp: {}", s),
            Err(e) => error!("resp error: {:?}", e),
        }
        future::ready(())
    });

    let miner_w = FramedWrite::new(w, codec);
    let miner_w = sc.fold(Ok(miner_w), async move |mw: Result<_, Box<dyn Error>>, msg| match mw {
        Ok(mut miner_w) => match miner_w.send(msg).timeout(timeout()).await {
            Ok(Ok(())) => Ok(miner_w),
            Ok(Err(e)) => Err(Box::new(e) as _),
            Err(e) => Err(Box::new(e) as _),
        },
        Err(e) => fatal!("unreachable#miner_w.fold Err2: {:?}", e),
    });

    match future::select(Box::pin(miner_w), miner_r).await {
        future::Either::Left((l, _)) => info!("#{} select finish left(w): {:?}", count, l?),
        future::Either::Right((r, _)) => info!("#{} select finish righ(r): {:?}", count, r),
    }

    Ok(())
}

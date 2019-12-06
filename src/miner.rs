use futures::{future, FutureExt, SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    runtime::Builder,
    sync::mpsc,
    time::timeout,
};
use tokio_rustls::webpki::DNSNameRef;
use tokio_util::codec::{Framed, LinesCodec, LinesCodecError};

use std::{thread, time::Instant};

use crate::{
    config::{timeout as timeoutv, Config},
    state::{Handler, ReqReceiver, State},
    util::{exited, sleep_secs, DescError, Result},
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
            let mut runtime = Builder::new().enable_all().basic_scheduler().build().expect("client Runtime new failed");

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

async fn connect<C, S>(state: &S, sc: &mut ReqReceiver, count: usize, start_time: &Instant) -> Result<()>
where
    S: Handler<C>,
{
    let socket = timeout(timeoutv(), TcpStream::connect(&state.config().pool.sa)).await??;
    info!("#{} tcp connect to {} ok", count, state.config().pool);

    if let Some((connector, domain)) = state.config().tls_config() {
        let domain = DNSNameRef::try_from_ascii_str(&domain)?;
        let socket = timeout(timeoutv(), connector.connect(domain, socket)).await??;
        info!("#{} tls connect to {} ok", count, state.config().pool);

        handle_socket(socket, state, sc, count, start_time).await
    } else {
        handle_socket(socket, state, sc, count, start_time).await
    }
}

async fn handle_socket<A, C, S>(socket: A, state: &S, sc: &mut ReqReceiver, count: usize, start_time: &Instant) -> Result<()>
where
    A: AsyncRead + AsyncWrite,
    S: Handler<C>,
{
    let codec = LinesCodec::new_with_max_length(1024);
    let (mut socket_w, socket_r) = Framed::new(socket, codec).split();

    // send login request
    let req = state.handle_request(state.login_request()).expect("handle_request(login_request)");
    timeout(timeoutv(), socket_w.send(req)).await??;

    let miner_r = loop_handle_response(socket_r, state);
    let miner_w = loop_handle_request(sc, socket_w, state, start_time);

    match future::select(Box::pin(miner_w), Box::pin(miner_r)).await {
        future::Either::Left((l, _)) => info!("#{} select finish left(w): {:?}", count, l?),
        future::Either::Right((r, _)) => info!("#{} select finish righ(r): {:?}", count, r?),
    }

    Ok(())
}

async fn loop_handle_response<C, S, R>(mut socket_r: R, state: &S) -> Result<()>
where
    S: Handler<C>,
    R: StreamExt<Item = std::result::Result<String, LinesCodecError>> + std::marker::Unpin,
{
    while let Some(msg) = socket_r.next().await {
        let resp = match msg {
            Ok(req) => req,
            Err(e) => {
                warn!("loop_handle_response failed: {}", e);
                return Err(e.into());
            }
        };
        if let Err(e) = state.handle_response(resp) {
            error!("resp error: {:?}", e)
        }
    }
    Ok(())
}

async fn loop_handle_request<C, S, W>(sc: &mut ReqReceiver, mut socket_w: W, state: &S, start_time: &Instant) -> Result<()>
where
    S: Handler<C>,
    W: SinkExt<String, Error = LinesCodecError> + std::marker::Unpin,
{
    while let Some(msg) = sc.recv().await {
        let req = match msg {
            Ok(req) => req,
            Err(e) => {
                let error_time: &Instant = e.as_ref();
                if error_time <= start_time {
                    warn!("skip error message belongs to the previous connection: {}", e);
                    continue;
                } else {
                    return Err(e.into());
                }
            }
        };
        let req = state.handle_request(req)?;
        let ok = timeout(timeoutv(), socket_w.send(req)).await?;
        if ok.is_err() {
            return Err(DescError::from("miner_w.send(msg).timeout()").into());
        }
    }

    Ok(())
}

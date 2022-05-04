use futures::{future, FutureExt, SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
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
    let (mp, mut sc) = mpsc::channel(512);
    let state: State<C> = State::new(config, mp);

    let state_clone = state.clone();
    let _client = thread::Builder::new()
        .name("net".into())
        .spawn(move || {
            let runtime = Builder::new_current_thread().enable_all().build().expect("client Runtime new failed");

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
    let config = state.config();
    let tls = config.tls_config();
    let socket = timeout(timeoutv(), connect_maybe_with_http_proxy(&config.pool.str, &config.pool.sa, tls.is_some())).await??;
    info!("#{} tcp connect to {} ok", count, config.pool);

    if let Some((connector, domain)) = tls {
        let domain = DNSNameRef::try_from_ascii_str(&domain)?;
        let socket = timeout(timeoutv(), connector.connect(domain, socket)).await??;
        info!("#{} tls connect to {} ok", count, state.config().pool);

        handle_socket(socket, state, sc, count, start_time).await
    } else {
        handle_socket(socket, state, sc, count, start_time).await
    }
}

async fn connect_maybe_with_http_proxy(dst: &str, dst_sa: &std::net::SocketAddr, tls: bool) -> Result<MaybleTlsStream> {
    let env_key = if tls { "https_proxy" } else { "http_proxy" };
    let env = std::env::var(env_key).ok();
    if let Some(proxy) = env {
        warn!("connect with {} ..", env_key);
        let url = url::Url::parse(&proxy)?;
        let https = url.scheme() == "https";

        let host = url.host_str().ok_or_else(|| format_err!("proxy invalid: without host"))?;
        let port = url.port().unwrap_or_else(|| if https { 443 } else { 80 });
        let host_port = format!("{}:{}", host, port);
        warn!("connect with {} {}://{:?} ..", proxy, host_port, url.scheme());

        let mut proxyc = TcpStream::connect(host_port).await?;
        let proxyc_addr = proxyc.peer_addr()?;
        info!("connect {} with proxy-{} ok: {}", dst, host, proxyc_addr);
        if https {
            let domain = url.host_str().ok_or_else(|| format_err!("proxy invalid: without domain"))?;
            let (tls_connector, domain) = Config::tls_config_for_proxy(Some(domain.to_owned())).unwrap();
            let mut proxyc = tls_connector.connect(DNSNameRef::try_from_ascii_str(&domain)?, proxyc).await?;
            info!("tls-handshake with proxy-{} ok: {}", host, proxyc_addr);
            handle_http_proxy(&mut proxyc, dst, &url).await?;
            Ok(MaybleTlsStream::Tls(proxyc))
        } else {
            handle_http_proxy(&mut proxyc, dst, &url).await?;
            Ok(MaybleTlsStream::Tcp(proxyc))
        }
    } else {
        let socket = TcpStream::connect(dst_sa).await?;
        Ok(MaybleTlsStream::Tcp(socket))
    }
}

async fn handle_http_proxy<A>(conn: &mut A, dst: &str, url: &url::Url) -> Result<()>
where
    A: AsyncReadExt + AsyncWriteExt + Unpin,
{
    let user = url.username();
    let pass = url.password().unwrap_or_default();

    let mut bytes = format!("CONNECT {} HTTP/1.1\r\n", dst);
    if user.len() + pass.len() > 0 {
        let auth = base64::encode(format!("{}:{}", user, pass));
        bytes.push_str("proxy-authorization: Basic ");
        bytes.push_str(&auth);
        bytes.push_str("\r\n");
    }
    bytes.push_str("\r\n");

    conn.write_all(bytes.as_bytes()).await?;

    let mut buf = [0u8; 128];
    let readc = conn.read(&mut buf).await?;
    let resp = std::str::from_utf8(&buf[..readc])?;
    let words = resp.split_whitespace().collect::<Vec<_>>();
    debug!("proxy.resp[..{}]: {}", readc, resp);

    if words[1] == "200" {
        return Ok(());
    }

    Err(format_err!("proxy responds !200: {} {}", words[1], words[2]))
}

pub enum MaybleTlsStream {
    Tcp(TcpStream),
    Tls(tokio_rustls::client::TlsStream<TcpStream>),
}

use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{self, ReadBuf};
impl AsyncRead for MaybleTlsStream {
    #[inline]
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(x) => Pin::new(x).poll_read(cx, buf),
            Self::Tls(x) => Pin::new(x).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybleTlsStream {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Tcp(x) => Pin::new(x).poll_write(cx, buf),
            Self::Tls(x) => Pin::new(x).poll_write(cx, buf),
        }
    }

    #[inline]
    fn poll_write_vectored(self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[std::io::IoSlice<'_>]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Tcp(x) => Pin::new(x).poll_write_vectored(cx, bufs),
            Self::Tls(x) => Pin::new(x).poll_write_vectored(cx, bufs),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match &self {
            Self::Tcp(x) => x.is_write_vectored(),
            Self::Tls(x) => x.is_write_vectored(),
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(x) => Pin::new(x).poll_flush(cx),
            Self::Tls(x) => Pin::new(x).poll_flush(cx),
        }
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(x) => Pin::new(x).poll_shutdown(cx),
            Self::Tls(x) => Pin::new(x).poll_shutdown(cx),
        }
    }
}

async fn handle_socket<A, C, S>(socket: A, state: &S, sc: &mut ReqReceiver, count: usize, start_time: &Instant) -> Result<()>
where
    A: AsyncRead + AsyncWrite,
    S: Handler<C>,
{
    let codec = LinesCodec::new_with_max_length(81920);
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

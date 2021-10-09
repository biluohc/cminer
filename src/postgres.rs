use std::convert::TryInto;
use std::io;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::*;
use tokio_rustls::TlsAcceptor;

use crate::common::{set_tcp_keepalive, try_copy};
use crate::config::Postgres;
use crate::state::State;

pub async fn handle_socket(
    mut socket: TcpStream,
    sa: std::net::SocketAddr,
    state: Arc<State<Postgres>>,
    acceptor: Option<TlsAcceptor>,
) -> io::Result<()> {
    let mut b1 = [0; 8];
    let mut b2 = [0; 8];

    let start = std::time::Instant::now();
    let n = socket.peek(&mut b1).await?;

    let a = u32::from_be_bytes(b1[..4].try_into().unwrap());
    let b = u32::from_be_bytes(b1[4..].try_into().unwrap());
    info!("{}: {:?}, {} {}", sa, b1, a, b);

    // 如果是 tls， 就答 S, 否则答 N
    let tls = acceptor.is_some();
    if a == 8 && b == 80877103 {
        // consume the data
        assert_eq!(n, socket.read(&mut b2[..n]).await?);

        let ans = if tls { "S" } else { "N" };
        socket.write(ans.as_bytes()).await?;

        let upstream = state
            .upstreams
            .iter()
            .min_by_key(|p| Arc::weak_count(&p))
            .unwrap();

        let _counter = Arc::downgrade(upstream);

        let mut socket2pg = TcpStream::connect(&upstream.url).await?;
        let socket2pg_sa = socket2pg.peer_addr()?;
        set_tcp_keepalive(&socket2pg, state.proxy.tcp_keepalive_secs)?;
        socket2pg.write(&b1).await?;
        socket2pg.read(&mut b2[..1]).await?;

        if b2[0] == b'N' {
            if let Some(tls) = acceptor {
                let socket = tls.accept(socket).await?;
                try_copy(socket, sa, socket2pg, socket2pg_sa, start, ans).await;
            } else {
                try_copy(socket, sa, socket2pg, socket2pg_sa, start, ans).await;
            }
        } else {
            error!("{}-{} connect to pg is enable tls, close it", sa, ans);
        }
    } else {
        error!("{} try peek failed costed {:?}", sa, start.elapsed());
    }

    Ok(())
}

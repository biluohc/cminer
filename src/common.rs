use bytes::{Bytes, BytesMut};
use futures::SinkExt;
use futures::StreamExt;
use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::codec::{BytesCodec, Framed};

use socket2::{SockRef, TcpKeepalive};

// https://github.com/tokio-rs/tokio/issues/3082
pub fn set_tcp_keepalive<'a, S: Into<SockRef<'a>>>(socket: S, secs: u64) -> io::Result<()> {
    if secs > 0 {
        let tcp_keepalive = TcpKeepalive::new().with_time(std::time::Duration::from_secs(secs));
        let socket_ref = socket.into();
        socket_ref.set_tcp_keepalive(&tcp_keepalive)?;
    }
    Ok(())
}

pub async fn try_copy<RW, RW2>(
    socket: RW,
    sa: std::net::SocketAddr,
    socket2pg: RW2,
    socket2pg_sa: std::net::SocketAddr,
    start: std::time::Instant,
    tls: &str,
) where
    RW: AsyncReadExt + AsyncWriteExt + std::marker::Unpin,
    RW2: AsyncReadExt + AsyncWriteExt + std::marker::Unpin,
{
    let (mut socket_w, mut socket_r) = Framed::new(socket, BytesCodec::new()).split();
    let (mut socket2pg_w, mut socket2pg_r) = Framed::new(socket2pg, BytesCodec::new()).split();

    match tokio::try_join! {
        copy(&mut socket_r, &mut socket2pg_w),
        copy(&mut socket2pg_r, &mut socket_w)
    } {
        Ok((up, down)) => {
            info!(
                "{}-{}<->{} finished costed {:?}, up: {}, down: {}",
                sa,
                tls,
                socket2pg_sa,
                start.elapsed(),
                up,
                down
            );
        }
        Err(e) => {
            info!(
                "{}-{}<->{} try join costed {:?}, failed: {}",
                sa,
                tls,
                socket2pg_sa,
                start.elapsed(),
                e
            );
        }
    }
}

async fn copy<R, W>(mut r: R, mut w: W) -> io::Result<u64>
where
    R: StreamExt<Item = std::result::Result<BytesMut, io::Error>> + std::marker::Unpin,
    W: SinkExt<Bytes, Error = io::Error> + std::marker::Unpin,
{
    let mut count = 0;
    while let Some(res) = r.next().await {
        let bs = res?.freeze();
        let size = bs.len();
        w.send(bs).await?;
        count += size as u64;
    }

    Ok(count)
}

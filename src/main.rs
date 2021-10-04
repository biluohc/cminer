#[macro_use]
extern crate serde;

use bytes::{Bytes, BytesMut};
use clap::Clap;
use futures::SinkExt;
use futures::StreamExt;
use std::convert::TryInto;
use std::fs::File;
use std::io::{self, BufReader};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::*;
use tokio_rustls::rustls::internal::pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use tokio_rustls::rustls::{Certificate, NoClientAuth, PrivateKey, ServerConfig};
use tokio_rustls::TlsAcceptor;
use tokio_util::codec::{BytesCodec, Framed};

#[derive(clap::Clap, Debug)]
pub struct Opts {
    // The number of occurrences of the `v/verbose` flag
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: u8,

    /// Config file
    #[clap(
        short = 'c',
        long = "config",
        parse(from_os_str),
        default_value = "pgproxy.toml"
    )]
    pub config: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PgProxy {
    pub listen: String,
    pub pg: String,
    pub cert: Option<String>,
    pub key: Option<String>,
}

fn load_certs<P: AsRef<Path>>(path: P) -> io::Result<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path.as_ref())?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
        .map(|mut certs| certs.drain(..).collect())
}

pub fn load_key(path: &str) -> io::Result<PrivateKey> {
    use std::io::Seek;

    let keyfile = std::fs::File::open(path)?;
    let mut reader = io::BufReader::new(keyfile);
    let mut keys = rsa_private_keys(&mut reader)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))?;

    if keys.is_empty() {
        reader.seek(io::SeekFrom::Start(0))?;
        keys = pkcs8_private_keys(&mut reader)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))?;
    }

    assert_eq!(keys.len(), 1);
    Ok(keys.remove(0))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opts::parse();
    println!("opt: {:?}", opt);
    let conf_str = std::fs::read_to_string(&opt.config)?;
    let conf = Arc::new(toml::from_str::<PgProxy>(&conf_str)?);

    let lis = conf
        .listen
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::from(io::ErrorKind::AddrNotAvailable))?;

    let mut acceptor = None;
    if conf.cert.is_some() && conf.key.is_some() {
        let certs = load_certs(conf.cert.as_ref().unwrap().as_str())?;
        let key = load_key(conf.key.as_ref().unwrap().as_str())?;

        let mut config = ServerConfig::new(NoClientAuth::new());
        config
            .set_single_cert(certs, key)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        acceptor = Some(TlsAcceptor::from(Arc::new(config)));
    }

    let listener = TcpListener::bind(lis).await?;
    loop {
        match listener.accept().await {
            Ok((socket, sa)) => {
                println!("accept {}", sa);
                tokio::spawn(handle_socket(socket, sa, conf.clone(), acceptor.clone()));
            }
            Err(e) => {
                println!("accept failed {}", e);
            }
        }
    }
}

async fn handle_socket(
    mut socket: TcpStream,
    sa: std::net::SocketAddr,
    config: Arc<PgProxy>,
    acceptor: Option<TlsAcceptor>,
) -> io::Result<()> {
    let mut b1 = [0; 8];
    let mut b2 = [0; 8];

    let start = std::time::Instant::now();
    let n = socket.peek(&mut b1).await?;

    let a = u32::from_be_bytes(b1[..4].try_into().unwrap());
    let b = u32::from_be_bytes(b1[4..].try_into().unwrap());
    println!("{}: {:?}, {} {}", sa, b1, a, b);

    // Read the data
    assert_eq!(n, socket.read(&mut b2[..n]).await?);
    assert_eq!(&b1[..n], &b2[..n]);

    // 如果是 tls， 就答 S, 否则答 N
    let tls = acceptor.is_some();
    if a == 8 && b == 80877103 {
        let ans = if tls { "S" } else { "N" };
        socket.write(ans.as_bytes()).await?;

        let mut socket2pg = TcpStream::connect(&config.pg).await?;
        let socket2pg_sa = socket2pg.peer_addr()?;
        socket2pg.write(&b1).await?;
        socket2pg.read(&mut b2[..1]).await?;

        if b2[0] == b'N' {
            if let Some(tls) = acceptor {
                let socket = tls.accept(socket).await?;
                try_copy(socket, sa, socket2pg, socket2pg_sa, start).await;
            } else {
                try_copy(socket, sa, socket2pg, socket2pg_sa, start).await;
            }
        } else {
            eprintln!("{} connect to pg is enable tls, close it", sa);
        }
    } else {
        eprintln!("{} try peek failed costed {:?}", sa, start.elapsed());
    }

    Ok(())
}

async fn try_copy<RW, RW2>(
    socket: RW,
    sa: std::net::SocketAddr,
    socket2pg: RW2,
    socket2pg_sa: std::net::SocketAddr,
    start: std::time::Instant,
) where
    RW: AsyncRead + AsyncWriteExt + std::marker::Unpin,
    RW2: AsyncRead + AsyncWriteExt + std::marker::Unpin,
{
    let (mut socket_w, mut socket_r) = Framed::new(socket, BytesCodec::new()).split();
    let (mut socket2pg_w, mut socket2pg_r) = Framed::new(socket2pg, BytesCodec::new()).split();

    match tokio::try_join! {
        copy(&mut socket_r, &mut socket2pg_w),
        copy(&mut socket2pg_r, &mut socket_w)
    } {
        Ok((up, down)) => {
            println!(
                "{}<->{} finished costed {:?}, up: {}, down: {}",
                sa,
                socket2pg_sa,
                start.elapsed(),
                up,
                down
            );
        }
        Err(e) => {
            println!(
                "{}<->{} try join costed {:?}, failed: {}",
                sa,
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

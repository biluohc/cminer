use crate::state::State;
use std::fs::File;
use std::io::{self, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_rustls::rustls::internal::pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use tokio_rustls::rustls::{Certificate, NoClientAuth, PrivateKey, ServerConfig};
use tokio_rustls::TlsAcceptor;

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
        default_value = "mypgproxy.toml"
    )]
    pub config: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Postgres {
    pub url: String,
    #[serde(skip)]
    pub force_tls: bool,
}

impl AsRef<str> for Postgres {
    fn as_ref(&self) -> &str {
        self.url.as_str()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Mysql {
    pub url: String,
    #[serde(skip)]
    pub force_tls: bool,
}

impl AsRef<str> for Mysql {
    fn as_ref(&self) -> &str {
        self.url.as_str()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Proxy {
    pub listen: SocketAddr,
    #[serde(default)]
    pub force_tls: bool,
    pub cert: Option<String>,
    pub key: Option<String>,
    #[serde(default)]
    pub postgres: Vec<Postgres>,
    #[serde(default)]
    pub mysql: Vec<Mysql>,
    #[serde(default)]
    pub workers: usize,
    #[serde(default)]
    pub metric_interval_secs: u64,
    #[serde(default)]
    pub tcp_keepalive_secs: u64,
}

impl Proxy {
    pub fn load_cert(&self) -> io::Result<Option<TlsAcceptor>> {
        if self.cert.is_some() && self.key.is_some() {
            let certs = load_certs(self.cert.as_ref().unwrap().as_str())?;
            let key = load_key(self.key.as_ref().unwrap().as_str())?;

            let mut config = ServerConfig::new(NoClientAuth::new());
            config
                .set_single_cert(certs, key)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
            let acceptor = TlsAcceptor::from(Arc::new(config));
            return Ok(Some(acceptor));
        }
        Ok(None)
    }

    pub fn to_postgres(self) -> State<Postgres> {
        let upstreams = self
            .postgres
            .clone()
            .into_iter()
            .map(|mut p| {
                p.force_tls = self.force_tls;
                Arc::new(p.clone())
            })
            .collect();

        State {
            upstreams,
            proxy: self,
        }
    }

    pub fn to_mysql(self) -> State<Mysql> {
        let upstreams = self
            .mysql
            .clone()
            .into_iter()
            .map(|mut p| {
                p.force_tls = self.force_tls;
                Arc::new(p.clone())
            })
            .collect();

        State {
            upstreams,
            proxy: self,
        }
    }
}

pub fn load_certs<P: AsRef<Path>>(path: P) -> io::Result<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path.as_ref())?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
        .map(|mut certs| certs.drain(..).collect())
}

pub fn load_key(path: &str) -> io::Result<PrivateKey> {
    use std::io::Seek;

    let keyfile = std::fs::File::open(path)?;
    let mut reader = io::BufReader::new(keyfile);
    let mut keys = rsa_private_keys(&mut reader)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid rsa key"))?;

    if keys.is_empty() {
        reader.seek(io::SeekFrom::Start(0))?;
        keys = pkcs8_private_keys(&mut reader)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid pem key"))?;
    }

    assert_eq!(keys.len(), 1);
    Ok(keys.remove(0))
}

use clap::arg_enum;
use nonblock_logger::log::LevelFilter::{self, *};

arg_enum! {
    #[derive(Debug, Clone, Copy)]
    pub enum Currency {
        Btc,
        Ckb,
        Eth,
    }
}

use std::{
    fmt,
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};

#[derive(Debug, Clone, StructOpt)]
pub struct PoolAddr {
    pub str: String,
    pub sa: SocketAddr,
}

impl fmt::Display for PoolAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.str, self.sa)
    }
}

impl std::str::FromStr for PoolAddr {
    type Err = String;
    fn from_str(pool: &str) -> Result<Self, Self::Err> {
        let mut iter = pool.to_socket_addrs().map_err(|e| format!("pool.to_socket_addrs failed: {:?}", e))?;
        iter.next().map(|sa| Self { str: pool.to_owned(), sa }).ok_or_else(|| "pool.to_socket_addrs is empty".into())
    }
}

#[derive(Debug, Clone, StructOpt)]
pub struct Config {
    #[structopt(short, long, help = "The address of pool: Host/IP:port")]
    pub pool: PoolAddr,
    #[structopt(long, default_value = "128", help = "Default is NumCPUs, if arg bigger than it, will reset as it")]
    pub workers: usize,
    #[structopt(short, long, default_value = "ckb")]
    #[structopt(possible_values = &Currency::variants(), case_insensitive = true, help ="Currency")]
    pub currency: Currency,
    #[structopt(short, long, default_value = "sp_yos", help = "User")]
    pub user: String,
    #[structopt(short, long, default_value = "0v0", help = "Name")]
    pub worker: String,
    #[structopt(short, long, default_value = "0", parse(from_occurrences), help = "Loglevel: -v(Info), -v -v(Debug), -v -v -v +(Trace)")]
    pub verbose: u8,
    #[structopt(short, long, default_value = "100", help = "program will reconnect if the job not updated for so many seconds")]
    pub expire: u64,
    #[structopt(short, long, help = "the domain for enable tls [An empty domain name means skipping the verify]")]
    pub domain: Option<String>,
}

impl Config {
    pub fn log(&self) -> LevelFilter {
        match self.verbose {
            0 => Warn,
            1 => Info,
            2 => Debug,
            _ => Trace,
        }
    }
    pub fn new<C, P, U, W>(currency: C, pool: P, workers: usize, user: U, worker: W, verbose: u8) -> Self
    where
        C: AsRef<str>,
        P: AsRef<str>,
        U: Into<String>,
        W: Into<String>,
    {
        Self {
            workers,
            verbose,
            expire: 100,
            domain: None,
            pool: pool.as_ref().parse().expect("resolve name failed"),
            currency: currency.as_ref().parse().unwrap_or(Currency::Eth),
            user: user.into(),
            worker: worker.into(),
        }
    }
    pub fn fix_workers(mut self) -> Self {
        let ws = num_cpus::get();
        if self.workers > ws {
            self.workers = ws;
        }
        self
    }
    pub fn tls_config(&self) -> Option<(TlsConnector, String)> {
        self.domain.clone().map(|mut d| {
            let mut config = ClientConfig::new();

            if d.is_empty() {
                config.dangerous().set_certificate_verifier(Arc::new(NoCertificateVerification));
                // "" will get InvalidDNSNameError
                d = "localhost".to_owned();
            } else {
                config.root_store.add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS);
            }
            (TlsConnector::from(Arc::new(config)), d)
        })
    }
}

use tokio_rustls::{rustls, rustls::ClientConfig, webpki, TlsConnector};

pub struct NoCertificateVerification;

impl rustls::ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _roots: &rustls::RootCertStore,
        _presented_certs: &[rustls::Certificate],
        _dns_name: webpki::DNSNameRef<'_>,
        _ocsp: &[u8],
    ) -> Result<rustls::ServerCertVerified, rustls::TLSError> {
        Ok(rustls::ServerCertVerified::assertion())
    }
}

pub const TIMEOUT_SECS: u64 = 3;

use std::time::Duration;
pub const fn timeout() -> Duration {
    Duration::from_secs(TIMEOUT_SECS)
}

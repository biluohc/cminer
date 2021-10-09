#[macro_use]
extern crate serde;
#[macro_use]
extern crate log;

use clap::Clap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Builder;
use tokio_rustls::TlsAcceptor;

pub mod config;
pub mod state;
use config::*;
use state::State;

pub mod common;
pub mod postgres;

fn main() -> io::Result<()> {
    let opt = Opts::parse();

    let key = "RUST_LOG";
    if std::env::var(key).is_err() {
        let l = match opt.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _more => "trace",
        };
        std::env::set_var(key, l);
    }
    env_logger::init();

    info!("opts: {:?}", opt);
    let conf_str = std::fs::read_to_string(&opt.config)?;
    debug!("conf:\n{}", conf_str);

    let conf = toml::from_str::<config::Proxy>(&conf_str)?;
    let acceptor = conf.load_cert()?;

    let rt = if conf.workers == 1 {
        Builder::new_current_thread().enable_all().build()?
    } else if conf.workers == 0 {
        Builder::new_multi_thread().enable_all().build()?
    } else {
        Builder::new_multi_thread()
            .worker_threads(conf.workers)
            .enable_all()
            .build()?
    };

    rt.block_on(fun(conf, acceptor))?;

    Ok(())
}

async fn fun(conf: Proxy, acceptor: Option<TlsAcceptor>) -> io::Result<()> {
    match (conf.postgres.is_empty(), conf.mysql.is_empty()) {
        (true, true) => error!("both postgres and mysql servers, exit"),
        (false, true) => {
            info!("running with postgres mode");
            let state = conf.to_postgres();

            return try_listen(state, acceptor, |socket, sa, s, a| {
                tokio::spawn(postgres::handle_socket(socket, sa, s, a));
            })
            .await;
        }
        (true, false) => {
            info!("running with mysql mode");
        }
        _ => error!("no servers, exit"),
    };

    std::process::exit(1)
}

async fn try_listen<T: AsRef<str>, F>(
    state: State<T>,
    acceptor: Option<TlsAcceptor>,
    handle_socket: F,
) -> std::io::Result<()>
where
    F: Fn(TcpStream, SocketAddr, Arc<State<T>>, Option<TlsAcceptor>) + 'static,
{
    let state = Arc::new(state);
    let listener = TcpListener::bind(state.proxy.listen).await?;
    let mut metric = tokio::time::interval(std::time::Duration::from_secs(
        state.proxy.metric_interval_secs,
    ));

    loop {
        tokio::select! {
            res = listener.accept() => match res {
                Ok((socket, sa)) => {
                    info!(
                        "accept {}, tls={}",
                        sa,
                        acceptor.is_some(),

                    );
                    handle_socket(socket, sa, state.clone(), acceptor.clone());
                }
                Err(e) => {
                    info!("accept failed {}", e);
                }
            },

            _ = metric.tick() => {
                info!("metric: {}", serde_json::to_string(&state.to_metric()).unwrap())
            }
        }
    }
}

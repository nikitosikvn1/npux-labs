#![allow(unused_imports)]
use std::{io, process};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use clap::{Parser, ValueHint};
use tracing_subscriber::EnvFilter;

use tcp_server::core::*;
use tcp_server::service::{Service, DelayedEchoService, FileTransferService};

const PROTOCOL_VERSION: u32 = 1;
const CHUNK_SIZE: usize = 1024;

#[derive(Parser, Debug)]
#[command(version = "1.0", about = "TCP server")]
struct Args {
    /// Socket address to bind to
    #[arg(short = 'a', long = "socket_addr", default_value = "127.0.0.1:7878")]
    socket_addr: SocketAddr,

    /// Base directory for file storage
    #[arg(short = 'd', long = "dir", value_hint = ValueHint::DirPath, default_value = "data")]
    base_dir: PathBuf,

    /// Number of worker threads for the thread pool server
    #[cfg(feature = "threadpool")]
    #[arg(short = 'w', long = "workers", default_value = "4")]
    workers: usize,

    /// Maximum number of child processes for the fork-per-connection server
    #[cfg(feature = "fork_per_connection")]
    #[arg(short = 'm', long = "max-processes", default_value = "4")]
    max_processes: usize,

    /// Number of preforked child processes for the prefork server
    #[cfg(feature = "prefork")]
    #[arg(short = 'p', long = "processes", default_value = "4")]
    processes: usize,
}

fn run_server(args: &Args, service: impl Service) -> io::Result<()> {
    #[cfg(not(any(feature = "threadpool", feature = "fork_per_connection", feature = "prefork")))]
    {
        let server = IterativeTcpServer::new(args.socket_addr, service)?;
        server.serve()
    }
    #[cfg(feature = "threadpool")]
    {
        let server = ThreadPoolTcpServer::new(args.socket_addr, service, args.workers)?;
        server.serve()
    }
    #[cfg(feature = "fork_per_connection")]
    {
        let server = ForkPerConnectionTcpServer::new(args.socket_addr, service, args.max_processes)?;
        server.serve()
    }
    #[cfg(feature = "prefork")]
    {
        let server = PreforkTcpServer::new(args.socket_addr, service, args.processes)?;
        server.serve()
    }
}

fn main() -> io::Result<()> {
    if cfg!(not(target_family = "unix")) {
        eprintln!("This program is intended for Unix-like systems only.");
        process::exit(1);
    }

    tracing_subscriber::fmt()
        .compact()
        .with_target(false)
        .with_env_filter(EnvFilter::from_default_env()) // RUST_LOG env var by default
        .init();

    let args = Args::parse();
    tracing::debug!(?args, "Parsed arguments");

    let ft_service = FileTransferService::new(&args.base_dir, PROTOCOL_VERSION, CHUNK_SIZE);

    run_server(&args, ft_service)
}

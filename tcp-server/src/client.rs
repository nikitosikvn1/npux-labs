use std::fs::File;
use std::path::PathBuf;
use std::net::SocketAddr;
use std::io::{self, BufWriter};

use clap::{Parser, ValueHint};
use tracing_subscriber::EnvFilter;

use tcp_server::service::FileTransferClient;
use tcp_server::proto::prelude::*;

const PROTOCOL_VERSION: u32 = 1;

#[derive(Parser, Debug)]
struct Args {
    /// Socket address to connect to
    #[arg(short = 'a', long = "socket_addr", default_value = "127.0.0.1:7878")]
    socket_addr: SocketAddr,

    /// File name to request from the server
    #[arg(short = 'f', long = "file")]
    file_name: String,

    /// Maximum file size to accept from the server
    #[arg(short = 's', long = "size", default_value_t = 8 * 1024 * 1024)] // Up to 8 MB by default
    max_file_size: u64,

    /// Directory to save the downloaded file
    #[arg(short = 'd', long = "dir", value_hint = ValueHint::DirPath, default_value = "downloads")]
    download_dir: PathBuf,
}

fn run_client(args: &Args) -> io::Result<()> {
    let file_path: PathBuf = args.download_dir.join(&args.file_name);
    let mut client = FileTransferClient::connect(args.socket_addr, PROTOCOL_VERSION)?;

    let file_response: FileResponse = client.request_file(&args.file_name)?;
    tracing::info!(?file_response, "Received FileResponse from server");

    match file_response.response {
        Some(Response::Metadata(metadata)) => {
            tracing::info!(?metadata, "Received file metadata");

            if metadata.file_size > args.max_file_size {
                tracing::error!(
                    "File size ({}) exceeds the maximum allowed size ({})",
                    metadata.file_size,
                    args.max_file_size
                );
                client.send_ack(AckStatus::Rejected)?;
                return Ok(());
            }
            client.send_ack(AckStatus::Accepted)?;

            let mut file: BufWriter<File> = BufWriter::new(File::create(&file_path)?);
            tracing::info!("Downloading file to {:?}", file_path);

            let bytes: u64 = client.receive_file(&mut file)?;
            tracing::info!(%bytes, "Received file data");
        }
        Some(Response::Error(details)) => tracing::error!(?details, "Server returned an error"),
        _ => tracing::error!("Server returned an unexpected response"),
    }

    Ok(())
}

fn main() -> io::Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .with_target(false)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    tracing::debug!(?args, "Parsed arguments");

    run_client(&args)
}

use std::thread;
use std::time::Duration;
use std::path::{Path, PathBuf};
use std::net::{TcpStream, Shutdown, ToSocketAddrs};
use std::fs::{self, File, Metadata};
use std::io::{self, Read, BufRead, Write, BufReader, BufWriter};
use tracing::instrument;
use prost::Message;

use crate::proto::prelude::*;

pub trait Service: Send + Sync + 'static {
    fn handle_connection(&self, stream: TcpStream) -> io::Result<()>;
}

/// A simple echo service that delays the echo response for a specified duration.
/// This service is primarily used for testing purposes.
pub struct DelayedEchoService {
    delay: Duration,
}

impl DelayedEchoService {
    pub fn new(delay: u64) -> Self {
        Self {
            delay: Duration::from_secs(delay),
        }
    }

    #[instrument(name = "echo_service", skip_all, fields(peer = ?stream.peer_addr().ok()))]
    pub fn handle_connection(&self, mut stream: TcpStream) -> io::Result<()> {
        let buf_reader: BufReader<&mut TcpStream> = BufReader::new(&mut stream);
        let data: Vec<String> = buf_reader
            .lines()
            .map(|line| line.unwrap())
            .take_while(|line| !line.is_empty())
            .collect();

        tracing::info!("Received data: {:#?}", data);
        thread::sleep(self.delay);

        let data: String = data.join("\n") + "\n";
        stream.write_all(data.as_bytes())?;

        stream.shutdown(Shutdown::Both)
    }
}

impl Service for DelayedEchoService {
    fn handle_connection(&self, stream: TcpStream) -> io::Result<()> {
        self.handle_connection(stream)
    }
}

/// Service that implements a simple file transfer protocol.
#[derive(Debug)]
pub struct FileTransferService {
    base_dir: PathBuf,
    protocol_version: u32,
    chunk_size: usize,
}

impl FileTransferService {
    /// Constructs a new `FileTransferService` with:
    /// - `base_dir`: the base directory where files are stored on the server.
    /// - `protocol_version`: the protocol version that the server supports.
    /// - `chunk_size`: the size of each file chunk to send to the client.
    pub fn new(base_dir: impl Into<PathBuf>, protocol_version: u32, chunk_size: usize) -> Self {
        Self {
            base_dir: base_dir.into(),
            protocol_version,
            chunk_size,
        }
    }

    /// Handles a single file transfer connection.
    /// The connection is expected to follow the file transfer protocol.
    #[instrument(name = "file_transfer_service", skip_all, fields(peer = ?stream.peer_addr().ok()))]
    pub fn handle_connection(&self, stream: &mut TcpStream) -> io::Result<()> {
        // 1. Read FileQuery message
        let query: FileQuery = self.read_file_query(stream)?;
        tracing::debug!(file_query = ?query, "Received FileQuery");
        self.verify_protocol_version(stream, &query)?;

        // 2. Write FileResponse message
        // TODO: !Possible directory traversal here!
        let file_path: PathBuf = self.base_dir.join(&query.filename);
        self.write_file_response(stream, &file_path)?;

        // 3. Read TransferAck message
        let ack: TransferAck = self.read_transfer_ack(stream)?;
        tracing::debug!(ack_status = ?AckStatus::try_from(ack.status).unwrap(), "Received ClientAck");

        // 4. Write FileChunk messages if the client accepted the file
        if ack.status == AckStatus::Accepted as i32 {
            self.write_file_chunks(stream, &file_path)?;
            tracing::debug!("File transfer complete");
        }
        tracing::debug!("Shutting down connection");

        self.shutdown(stream)
    }

    /// Reads a `FileQuery` message from the stream.
    ///
    /// The message is expected to be length-delimited (with a 4-byte big-endian length prefix).
    /// If decoding fails, the connection is shut down and an error is returned.
    fn read_file_query(&self, stream: &mut TcpStream) -> io::Result<FileQuery> {
        read_message::<FileQuery>(stream).or_else(|e| {
            self.shutdown(stream)?;

            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to read FileQuery: {}", e),
            ))
        })
    }

    /// Verifies that the protocol version in the `FileQuery` matches the server's version.
    ///
    /// If the versions do not match, an error message is sent (with `UNSUPPORTED_VERSION`)
    /// and the connection is closed. An error is returned in this case.
    fn verify_protocol_version(&self, stream: &mut TcpStream, query: &FileQuery) -> io::Result<()> {
        if query.version != self.protocol_version {
            let message: String = format!(
                "Protocol version mismatch: server={:?}, client={:?}",
                self.protocol_version, query.version,
            );
            self.write_error_and_shutdown(stream, Kind::UnsupportedVersion, &message)?;

            Err(io::Error::new(io::ErrorKind::Unsupported, message))?
        }

        Ok(())
    }

    /// Writes a `FileResponse` message with file metadata to the stream.
    ///
    /// If the file exists, the response status will be `FOUND` along with the file's size.
    /// Otherwise, the status will be `NOT_FOUND`. If the file does not exist, the connection is
    /// shut down and an error is returned. The response is sent as a length-delimited
    /// message (with a 4-byte big-endian length prefix).
    fn write_file_response(&self, stream: &mut TcpStream, file_path: &Path) -> io::Result<()> {
        let metadata: Option<Metadata> = fs::metadata(file_path).ok();
        let file_metadata = FileMetadata {
            status: match metadata {
                Some(_) => Status::Found as i32,
                None => Status::NotFound as i32,
            },
            file_size: metadata.as_ref().map_or(0, |m| m.len()),
        };
        let response = FileResponse {
            response: Some(Response::Metadata(file_metadata)),
        };
        write_message(stream, &response)?;

        if metadata.is_none() {
            self.shutdown(stream)?;

            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("File not found: {:?}", file_path),
            ))?;
        }

        Ok(())
    }

    /// Reads a `TransferAck` message from the stream.
    ///
    /// The message is expected to be length-delimited (with a 4-byte big-endian length prefix).
    /// If decoding fails, the connection is shut down and an error is returned.
    fn read_transfer_ack(&self, stream: &mut TcpStream) -> io::Result<TransferAck> {
        read_message::<TransferAck>(stream).or_else(|e| {
            self.shutdown(stream)?;

            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to read TransferAck: {}", e),
            ))
        })
    }

    /// Sends the file to the client in chunks.
    ///
    /// The file is opened for reading and is split into chunks of size `self.chunk_size`.
    /// Each chunk is wrapped in a `FileChunk` message and sent using a length-delimited format.
    /// Once all chunks have been sent, the writer is flushed.
    fn write_file_chunks(&self, stream: &mut TcpStream, file_path: &Path) -> io::Result<()> {
        let mut writer: BufWriter<&mut TcpStream> = BufWriter::new(stream);
        let mut file: BufReader<File> = BufReader::new(File::open(file_path)?);

        let mut index: u32 = 0;
        let mut buf: Vec<u8> = vec![0; self.chunk_size];

        loop {
            let bytes_read: usize = file.read(&mut buf)?;
            if bytes_read == 0 {
                break; // EOF
            }
            let file_chunk = FileChunk {
                index,
                data: buf[..bytes_read].to_vec(),
            };
            write_message(&mut writer, &file_chunk)?;
            index += 1;
        }

        writer.flush()
    }

    /// Sends an error message to the client and then shuts down the connection.
    fn write_error_and_shutdown(&self, stream: &mut TcpStream, kind: Kind, message: &str) -> io::Result<()> {
        let error_info = ErrorDetails {
            kind: kind as i32,
            message: message.to_string(),
        };
        let response = FileResponse {
            response: Some(Response::Error(error_info)),
        };
        write_message(stream, &response)?;

        self.shutdown(stream)
    }

    /// Shuts down the connection.
    fn shutdown(&self, stream: &mut TcpStream) -> io::Result<()> {
        stream.shutdown(Shutdown::Both)
    }
}

impl Service for FileTransferService {
    fn handle_connection(&self, mut stream: TcpStream) -> io::Result<()> {
        self.handle_connection(&mut stream)
    }
}

/// A client part of the file transfer protocol.
pub struct FileTransferClient {
    stream: TcpStream,
    protocol_version: u32,
}

impl FileTransferClient {
    /// Connects to the server at the specified address and returns a new `FileTransferClient`.
    pub fn connect(addr: impl ToSocketAddrs, protocol_version: u32) -> io::Result<Self> {
        Ok(Self {
            stream: TcpStream::connect(addr)?,
            protocol_version,
        })
    }

    /// Requests a file with the specified name from the server.
    pub fn request_file(&mut self, filename: &str) -> io::Result<FileResponse> {
        let query = FileQuery {
            version: self.protocol_version,
            filename: filename.to_string(),
        };
        self.write_message(&query)?;

        let response: FileResponse = self.read_message()?;

        Ok(response)
    }

    /// Sends an acknowledgment to the server with the specified status.
    pub fn send_ack(&mut self, status: AckStatus) -> io::Result<()> {
        let ack = TransferAck {
            status: status as i32,
        };

        self.write_message(&ack)
    }

    /// Receives a file from the server and writes it to the specified writer.
    pub fn receive_file(&mut self, writer: &mut impl Write) -> io::Result<u64> {
        let mut total_bytes_received: u64 = 0;

        loop {
            match self.read_message::<FileChunk>() {
                Ok(chunk) => {
                    tracing::trace!(index = chunk.index, "Received file chunk");

                    writer.write_all(&chunk.data)?;
                    total_bytes_received += chunk.data.len() as u64;
                }
                Err(e) => {
                    if e.kind() == io::ErrorKind::UnexpectedEof {
                        break;
                    }
                    return Err(e);
                }
            }
        }
        writer.flush()?;

        Ok(total_bytes_received)
    }

    fn write_message(&mut self, message: &impl Message) -> io::Result<()> {
        write_message(&mut self.stream, message)
    }

    fn read_message<M: Message + Default>(&mut self) -> io::Result<M> {
        read_message(&mut self.stream)
    }
}

/// Reads a length-delimited message from the reader.
/// The message must be prefixed with a 4-byte length (big-endian):
///
/// | message length (4 bytes, big-endian) | message bytes |
fn read_message<M: Message + Default>(reader: &mut impl Read) -> io::Result<M> {
    let mut len_buf: [u8; 4] = [0; 4];
    reader.read_exact(&mut len_buf)?;
    let message_len = u32::from_be_bytes(len_buf);

    let mut message_buf: Vec<u8> = vec![0; message_len as usize];
    reader.read_exact(&mut message_buf)?;

    M::decode(&*message_buf).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to decode Protobuf message: {}", e),
        )
    })
}

/// Writes a length-delimited message to the writer.
/// The message is encoded to bytes and prefixed with a 4-byte length (big-endian):
///
/// | message length (4 bytes, big-endian) | message bytes |
fn write_message(writer: &mut impl Write, message: &impl Message) -> io::Result<()> {
    let message_bytes: Vec<u8> = message.encode_to_vec();
    let message_len: u32 = message_bytes.len() as u32;

    writer.write_all(&message_len.to_be_bytes())?;
    writer.write_all(&message_bytes)?;

    Ok(())
}

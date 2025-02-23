use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::os::fd::{RawFd, AsRawFd as _};
use libc::{pid_t, c_int, WNOHANG, SIGTERM, PR_SET_PDEATHSIG};
use tracing::instrument;

use crate::service::Service;
use crate::thread_pool::ThreadPool;

/// Base TCP server that listens on a given socket address.
/// Used as a building block for other server types.
struct BaseTcpServer {
    listener: TcpListener,
}

impl BaseTcpServer {
    fn bind(socket_addr: impl ToSocketAddrs) -> io::Result<Self> {
        let listener: TcpListener = TcpListener::bind(socket_addr)?;

        Ok(Self { listener })
    }

    fn init(&self) -> io::Result<()> {
        tracing::info!("Listening on {}...", self.listener.local_addr()?);

        Ok(())
    }

    #[instrument(name = "server", skip_all)]
    fn run_accept_loop<F>(&self, connection_handler: F) -> io::Result<()>
    where
        F: Fn(TcpStream),
    {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Ok(peer) = stream.peer_addr() {
                        tracing::info!(peer_addr = ?peer, "Accepted connection");
                    }
                    connection_handler(stream);
                }
                Err(e) => tracing::error!("Failed to establish a connection: {}", e),
            }
        }

        Ok(())
    }

    fn close_listener(&self) {
        let listener_fd: RawFd = self.listener.as_raw_fd();
        unsafe { libc::close(listener_fd) };
    }
}

/// Iterative TCP server that handles one connection at a time.
pub struct IterativeTcpServer<S: Service> {
    service: S,
    server: BaseTcpServer,
}

impl<S: Service> IterativeTcpServer<S> {
    pub fn new(socket_addr: impl ToSocketAddrs, service: S) -> io::Result<Self> {
        Ok(Self {
            service,
            server: BaseTcpServer::bind(socket_addr)?,
        })
    }

    pub fn serve(&self) -> io::Result<()> {
        self.server.init()?;

        self.server.run_accept_loop(|stream| {
            if let Err(e) = self.service.handle_connection(stream) {
                tracing::error!("Service error: failed to handle connection: {}", e);
            }
        })
    }
}

/// Thread pool-based TCP server that handles multiple connections concurrently.
pub struct ThreadPoolTcpServer<S: Service> {
    service: Arc<S>,
    server: BaseTcpServer,
    pool: ThreadPool,
}

impl<S: Service> ThreadPoolTcpServer<S> {
    pub fn new(socket_addr: impl ToSocketAddrs, service: S, num_workers: usize) -> io::Result<Self> {
        Ok(Self {
            service: Arc::new(service),
            server: BaseTcpServer::bind(socket_addr)?,
            pool: ThreadPool::new(num_workers),
        })
    }

    pub fn serve(&self) -> io::Result<()> {
        self.server.init()?;

        self.server.run_accept_loop(|stream| {
            let service: Arc<S> = Arc::clone(&self.service);

            self.pool.execute(move || {
                if let Err(e) = service.handle_connection(stream) {
                    tracing::error!("Service error: failed to handle connection: {}", e);
                }
            });
        })
    }
}

/// Fork-per-connection TCP server that forks a new process for each incoming connection.
pub struct ForkPerConnectionTcpServer<S: Service> {
    service: S,
    server: BaseTcpServer,
    max_children: usize,
    active_children: AtomicUsize,
}

impl<S: Service> ForkPerConnectionTcpServer<S> {
    pub fn new(socket_addr: impl ToSocketAddrs, service: S, max_children: usize) -> io::Result<Self> {
        Ok(Self {
            service,
            server: BaseTcpServer::bind(socket_addr)?,
            max_children,
            active_children: AtomicUsize::new(0),
        })
    }

    pub fn serve(&self) -> io::Result<()> {
        self.server.init()?;

        self.server.run_accept_loop(|stream| {
            self.cleanup_finished_children();
            self.wait_for_available_slot();

            match unsafe { libc::fork() } {
                0 => {
                    self.run_child_process(stream);
                    unsafe { libc::_exit(0) };
                }
                pid if pid > 0 => {
                    self.active_children.fetch_add(1, Ordering::Relaxed);
                    tracing::info!(%pid, active = self.active_children.load(Ordering::Relaxed), "Forked child");
                }
                _ => tracing::error!("Failed to fork a child process"),
            }
        })
    }

    #[instrument(name = "child", skip_all, fields(pid = unsafe { libc::getpid() }))]
    fn run_child_process(&self, stream: TcpStream) {
        self.server.close_listener();

        if let Err(e) = self.service.handle_connection(stream) {
            tracing::error!("Service error: failed to handle connection: {}", e);
        }
    }

    fn cleanup_finished_children(&self) {
        while let Ok(Some((pid, status))) = wait_child(true) {
            tracing::info!(%pid, %status, "Child exited");
            self.active_children.fetch_sub(1, Ordering::Relaxed);
        }
    }

    fn wait_for_available_slot(&self) {
        while self.active_children.load(Ordering::Relaxed) >= self.max_children {
            tracing::warn!("Reached the maximum number of children. Waiting for a child to exit...");

            if let Err(e) = wait_child(false) {
                tracing::error!("Failed to wait for a child: {}", e);
            } else {
                self.active_children.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }
}

/// Prefork TCP server that forks a fixed number of child processes to handle incoming connections.
pub struct PreforkTcpServer<S: Service> {
    service: S,
    server: BaseTcpServer,
    num_children: usize,
}

impl<S: Service> PreforkTcpServer<S> {
    pub fn new(socket_addr: impl ToSocketAddrs, service: S, num_children: usize) -> io::Result<Self> {
        Ok(Self {
            service,
            server: BaseTcpServer::bind(socket_addr)?,
            num_children,
        })
    }

    pub fn serve(&self) -> io::Result<()> {
        self.server.init()?;

        for _ in 0..self.num_children {
            match unsafe { libc::fork() } {
                0 => {
                    if let Err(e) = self.run_child_process() {
                        tracing::error!("Child process failed: {}", e);
                    }
                    unsafe { libc::_exit(0) };
                }
                pid if pid > 0 => tracing::info!(%pid, "Forked child process"),
                _ => tracing::error!("Failed to fork a child process"),
            }
        }
        unsafe { libc::pause() }; // Just wait for a signal

        Ok(())
    }

    #[instrument(name = "child", skip_all, fields(pid = unsafe { libc::getpid() }))]
    fn run_child_process(&self) -> io::Result<()> {
        // Not the most graceful shutdown
        if unsafe { libc::prctl(PR_SET_PDEATHSIG, SIGTERM) } != 0 {
            tracing::error!("Failed to set PR_SET_PDEATHSIG: {}", io::Error::last_os_error());
            unsafe { libc::_exit(1) };
        }

        self.server.run_accept_loop(|stream| {
            if let Err(e) = self.service.handle_connection(stream) {
                tracing::error!("Service error: failed to handle connection: {}", e);
            }
        })
    }
}

fn wait_child(non_blocking: bool) -> io::Result<Option<(pid_t, c_int)>> {
    let mut status: c_int = 0;
    let options: c_int = if non_blocking { WNOHANG } else { 0 };

    match unsafe { libc::waitpid(-1, &mut status as *mut _, options) } {
        0 => Ok(None),
        -1 => Err(io::Error::last_os_error()),
        pid => Ok(Some((pid, status))),
    }
}

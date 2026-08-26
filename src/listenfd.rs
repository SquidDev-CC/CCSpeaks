use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use std::borrow::Borrow;
use std::error::Error as StdError;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::str::FromStr;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};

use hyper::body::{Body, Incoming as IncomingBody};
use hyper::service::HttpService;
use rustix::net::{AddressFamily, getsockname};

/// A possible socket passed from systemd. This can either be a TCP socket
/// (when `ListenStream` is a IP address and port) or a Unix socket (when
/// `ListenStream` is a path).
enum Socket {
  Tcp(TcpListener),
  Unix(UnixListener),
}

/// A type that can accept connections, returning a stream that may be
/// read/written from.
trait Listener {
  type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

  async fn accept_connection(&mut self) -> std::io::Result<Self::Stream>;
}

impl Listener for TcpListener {
  type Stream = TcpStream;

  async fn accept_connection(&mut self) -> std::io::Result<Self::Stream> {
    self.accept().await.map(|(stream, _)| stream)
  }
}

impl Listener for UnixListener {
  type Stream = UnixStream;

  async fn accept_connection(&mut self) -> std::io::Result<Self::Stream> {
    self.accept().await.map(|(stream, _)| stream)
  }
}

/// Serve connections using the provided listener.
async fn run_server<L, S>(mut listener: L, service: S) -> anyhow::Result<()>
where
  L: Listener,
  S: HttpService<IncomingBody> + Send + Clone + 'static,
  S::Error: Into<Box<dyn StdError + Send + Sync>>,
  S::Future: Send,
  S::ResBody: Send + 'static,
  <S::ResBody as Body>::Data: Send,
  <S::ResBody as Body>::Error: Into<Box<dyn StdError + Send + Sync>>,
{
  loop {
    let stream = listener.accept_connection().await?;
    let io = TokioIo::new(stream);
    let service = service.clone();
    tokio::task::spawn(async move {
      if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
        eprintln!("Error serving connection: {:?}", err);
      }
    });
  }
}

/// Parse an environment variable.
fn get_env<T: FromStr>(name: &str) -> std::io::Result<Option<T>> {
  std::env::var_os(name)
    .map(|value| {
      value
        .into_string()
        .ok()
        .and_then(|x| x.parse().ok())
        .ok_or_else(|| {
          io::Error::new(io::ErrorKind::InvalidInput, format!("Cannot parse {:?}", name))
        })
    })
    .transpose()
}

/// Extract a socket passed via systemd's socket passing interface, falling back
/// to a `LISTEN_PORT` variable if not available.
///
/// See https://www.freedesktop.org/software/systemd/man/latest/sd_listen_fds.html
async fn get_socket() -> std::io::Result<Socket> {
  if let Some(listen_fds) = get_env::<u32>("LISTEN_FDS")? {
    if listen_fds < 1 {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "Process started with no listen file descriptors",
      ));
    }

    // Check our current process is equal to LISTEN_PID.
    if let Some(listen_pid) = get_env::<u32>("LISTEN_PID")?
      && listen_pid != std::process::id()
    {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "LISTEN_PID is for the wrong process",
      ));
    };

    let fd = unsafe { OwnedFd::from_raw_fd(3) };

    match getsockname(fd.borrow())?.address_family() {
      AddressFamily::INET | AddressFamily::INET6 => {
        let listener = std::net::TcpListener::from(fd);
        listener.set_nonblocking(true)?;
        Ok(Socket::Tcp(TcpListener::from_std(listener)?))
      }
      AddressFamily::UNIX => {
        let listener = std::os::unix::net::UnixListener::from(fd);
        listener.set_nonblocking(true)?;
        Ok(Socket::Unix(UnixListener::from_std(listener)?))
      }
      af => Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("fd {} has an unknown address family {:?}", fd.as_raw_fd(), af),
      )),
    }
  } else if let Some(listen_port) = get_env("LISTEN_PORT")? {
    Ok(Socket::Tcp(TcpListener::bind(("127.0.0.1", listen_port)).await?))
  } else {
    Err(io::Error::new(io::ErrorKind::InvalidInput, "Must run with LISTENFD or LISTEN_PORT"))
  }
}

/// Serve a HTTP service on the systemd socket.
pub async fn serve<S>(service: S) -> anyhow::Result<()>
where
  S: HttpService<IncomingBody> + Send + Clone + 'static,
  S::Error: Into<Box<dyn StdError + Send + Sync>>,
  S::Future: Send,
  S::ResBody: Send + 'static,
  <S::ResBody as Body>::Data: Send,
  <S::ResBody as Body>::Error: Into<Box<dyn StdError + Send + Sync>>,
{
  match get_socket().await? {
    Socket::Tcp(listener) => run_server(listener, service).await,
    Socket::Unix(listener) => run_server(listener, service).await,
  }
}

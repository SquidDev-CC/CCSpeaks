use anyhow::Result;
use hyper::server::conn::AddrStream;
use hyper::server::Server;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, StatusCode};
use listenfd::ListenFd;
use log::info;
use parking_lot::Mutex;
use std::convert::Infallible;
use tracing::{event, span, Level as TLevel};
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::fmt::format::FmtSpan;

mod audio;
mod speak;

const MAX_SIZE: usize = 512;

fn bad_request(message: &'static str) -> Response<Body> {
  Response::builder()
    .status(StatusCode::BAD_REQUEST)
    .body(message.into())
    .unwrap()
}

fn handle_request(
  speak: &Mutex<speak::Speak>,
  sample_rate: usize,
  request: Request<Body>,
) -> Response<Body> {
  let text = request
    .uri()
    .query()
    .and_then(|x| form_urlencoded::parse(x.as_bytes()).find(|(k, _)| k == "text"))
    .map(|(_, v)| v);
  let text = match text {
    None => return bad_request("No text= query parameter"),
    Some(x) => x,
  };

  if text.len() > MAX_SIZE {
    return bad_request("Text is too long.");
  } else if !text.is_ascii() {
    return bad_request("Text must be ASCII only.");
  }

  info!("Speaking {:?}", text);
  let wav = match speak.lock().speak(&text) {
    Ok(result) => result,
    Err(e) => {
      return Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(format!("Failed to generate audio ({})", e).into())
        .unwrap()
    }
  };

  use audio::AudioIterator;
  let result: Vec<u8> = wav
    .iter()
    .map(|x| (*x as f64) / (i16::MAX as f64))
    .resample(sample_rate as usize, 48_000)
    .map(|x| (x * 127.0).floor() as i8)
    .to_dfpwm()
    .collect();

  Response::builder().body(result.into()).unwrap()
}

#[tokio::main]
async fn main() -> Result<()> {
  tracing_subscriber::fmt()
    .with_env_filter(
      EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy(),
    )
    .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
    .init();

  let speaker = speak::Speak::init();
  let sample_rate = speaker.sample_rate() as usize;
  let speaker = std::sync::Arc::new(parking_lot::Mutex::new(speaker));

  // Look, I don't even know.
  let make_svc = make_service_fn(|conn: &AddrStream| {
    let speaker = speaker.clone();
    let addr = conn.remote_addr().to_string();
    let fun = service_fn(move |req| {
      let span = span!(TLevel::INFO, "request", addr = addr);
      let _enter = span.enter();

      let res = handle_request(&speaker, sample_rate, req);
      event!(TLevel::INFO, status = res.status().as_str());
      async move { Ok::<_, Infallible>(res) }
    });

    async move { Ok::<_, Infallible>(fun) }
  });

  // Bind using systemd sockets or an explicitly provided port.
  let server = if let Some(l) = ListenFd::from_env().take_tcp_listener(0)? {
    Server::from_tcp(l)?
  } else if let Some(port) = std::env::var("LISTEN_PORT")
    .ok()
    .and_then(|x| x.parse().ok())
  {
    Server::bind(&([127, 0, 0, 1], port).into())
  } else {
    anyhow::bail!("Must run with LISTENFD or LISTEN_PORT!")
  };

  server.serve(make_svc).await?;

  Ok(())
}

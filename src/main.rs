use anyhow::Result;
use hyper::server::conn::AddrStream;
use hyper::server::Server;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, StatusCode};
use listenfd::ListenFd;
use log::info;
use opentelemetry::global;
use opentelemetry::trace::{Span, SpanBuilder, SpanKind, Tracer};
use opentelemetry::KeyValue;
use parking_lot::Mutex;
use std::convert::Infallible;

mod audio;
mod monitoring;
mod speak;

const MAX_SIZE: usize = 512;

fn bad_request(message: &'static str) -> Response<Body> {
  Response::builder()
    .status(StatusCode::BAD_REQUEST)
    .body(message.into())
    .unwrap()
}

fn handle_request(speak: &Mutex<speak::Speak>, request: Request<Body>) -> Response<Body> {
  let mut text = None;
  let mut voice = None;
  for (k, v) in form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()) {
    match k.as_ref() {
      "text" => text = Some(v),
      "voice" => voice = Some(v),
      _ => return bad_request("Unknown query argument"),
    }
  }

  let Some(text) = text.as_deref() else {
    return bad_request("No text= query parameter")
  };
  let voice = voice.as_deref().unwrap_or(speak::DEFAULT_VOICE);

  if text.len() > MAX_SIZE {
    return bad_request("Text is too long.");
  } else if text.contains('\0') {
    return bad_request("Text cannot contain special characters.");
  }

  if voice.len() > MAX_SIZE {
    return bad_request("Voice is too long.");
  } else if !voice.is_ascii() {
    return bad_request("Voice must be ASCII only.");
  }

  info!("Speaking {:?} with {:?}", text, voice);
  let (sample_rate, wav) = {
    let mut speak = speak.lock();

    let sample_rate = match speak.set_voice(voice) {
      Ok(rate) => rate,
      Err(err) => return bad_request(err),
    };

    match speak.speak(text) {
      Ok(result) => (sample_rate, result),
      Err(e) => {
        return Response::builder()
          .status(StatusCode::INTERNAL_SERVER_ERROR)
          .body(format!("Failed to generate audio ({})", e).into())
          .unwrap()
      }
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
  monitoring::setup()?;

  let speaker = speak::Speak::init();
  let speaker = std::sync::Arc::new(parking_lot::Mutex::new(speaker));

  // Look, I don't even know.
  let make_svc = make_service_fn(|_conn: &AddrStream| {
    let speaker = speaker.clone();
    let fun = service_fn(move |req| {
      let parent_cx = global::get_text_map_propagator(|propagator| {
        propagator.extract(&opentelemetry_http::HeaderExtractor(req.headers()))
      });
      let mut span = global::tracer(monitoring::SERVICE_NAME).build_with_context(
        SpanBuilder::from_name("speak").with_kind(SpanKind::Server),
        &parent_cx,
      );

      let res = handle_request(&speaker, req);

      span.set_attribute(KeyValue::new(
        opentelemetry_semantic_conventions::trace::HTTP_STATUS_CODE,
        res.status().as_u16() as i64,
      ));

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

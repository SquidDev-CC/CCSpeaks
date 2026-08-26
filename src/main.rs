use anyhow::Result;
use http_body_util::Full;
use hyper::body;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use std::convert::Infallible;
use std::sync::Mutex;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod audio;
mod listenfd;
mod speak;

const MAX_SIZE: usize = 512;

fn bad_request(message: &'static str) -> Response<Full<body::Bytes>> {
  Response::builder()
    .status(StatusCode::BAD_REQUEST)
    .body(message.into())
    .unwrap()
}

fn handle_request(
  speak: &Mutex<speak::Speak>,
  request: Request<body::Incoming>,
) -> Response<Full<body::Bytes>> {
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
    return bad_request("No text= query parameter");
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

  tracing::info!("Speaking {:?} with {:?}", text, voice);
  let (sample_rate, wav) = {
    let mut speak = speak.lock().unwrap();

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
          .unwrap();
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
  tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    .with(
      EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy(),
    )
    .init();

  let speaker = speak::Speak::init();
  let speaker = std::sync::Arc::new(Mutex::new(speaker));

  let service = service_fn(move |req| {
    let span = tracing::info_span!(
      "Request",
      http.request.method = %req.method(),
      http.request.path = req.uri().path(),
      http.response.status_code = tracing::field::Empty,
      http.response.body.size = tracing::field::Empty,
    )
    .entered();
    let res = handle_request(&speaker, req);
    span.record("http.response.status_code", res.status().as_u16());
    async move { Ok::<_, Infallible>(res) }
  });

  crate::listenfd::serve(service).await?;

  Ok(())
}

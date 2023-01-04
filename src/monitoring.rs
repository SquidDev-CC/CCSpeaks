use anyhow::Result;
use opentelemetry::propagation::text_map_propagator::FieldIter;
use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator};
use opentelemetry::sdk::trace;
use opentelemetry::sdk::trace::Sampler;
use opentelemetry::sdk::Resource;
use opentelemetry::trace::{SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState};
use opentelemetry::{global, Context, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use std::str::FromStr;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub const SERVICE_NAME: &str = "music.madefor.cc";

static JAEGER_FIELD: &str = "uber-trace-id";

/// A [`TextMapPropagator`] which extracts Jaeger traces instead.
///
/// Ideally we'd use W3C's trace format, but my current web server doesn't
/// support this.
#[derive(Debug)]
struct JaegerExtractor([String; 1]);

impl JaegerExtractor {
  fn new() -> Self {
    JaegerExtractor([JAEGER_FIELD.to_owned()])
  }
}

impl TextMapPropagator for JaegerExtractor {
  fn inject_context(&self, _cx: &Context, _injector: &mut dyn Injector) {}

  fn extract_with_context(&self, cx: &Context, extractor: &dyn Extractor) -> Context {
    fn get_context(extractor: &dyn Extractor) -> Option<SpanContext> {
      let header_value = extractor.get(JAEGER_FIELD).unwrap_or("");
      let parts = header_value.split_terminator(':').collect::<Vec<&str>>();
      if parts.len() != 4 {
        return None;
      }

      // extract trace id
      let trace_id = TraceId::from_hex(parts[0]).ok()?;
      let span_id = SpanId::from_hex(parts[1]).ok()?;
      let flags = u8::from_str(parts[3]).ok()?;
      let flags = if (flags & 0x01) != 0 {
        TraceFlags::SAMPLED
      } else {
        TraceFlags::default()
      };
      Some(SpanContext::new(trace_id, span_id, flags, true, TraceState::default()))
    }

    match get_context(extractor) {
      None => cx.clone(),
      Some(remote_cx) => cx.with_remote_span_context(remote_cx),
    }
  }

  fn fields(&self) -> FieldIter<'_> {
    FieldIter::new(&self.0)
  }
}

/// Configure OpenTracing and the `tracing` crate.
pub fn setup() -> Result<()> {
  // Setup our OpenTelemetry tracer.
  global::set_text_map_propagator(JaegerExtractor::new());

  let trace_config = trace::config()
    .with_sampler(Sampler::AlwaysOn)
    .with_resource(Resource::new(vec![KeyValue::new(
      opentelemetry_semantic_conventions::resource::SERVICE_NAME,
      "ccspeaks",
    )]));

  let tracer = if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
    opentelemetry_otlp::new_pipeline()
      .tracing()
      .with_exporter(opentelemetry_otlp::new_exporter().tonic().with_env())
      .with_trace_config(trace_config)
      .install_batch(opentelemetry::runtime::Tokio)?
  } else {
    opentelemetry::sdk::export::trace::stdout::new_pipeline()
      .with_trace_config(trace_config)
      .install_simple()
  };

  // And then set up the tracing one.
  tracing_subscriber::registry()
    .with(tracing_opentelemetry::layer().with_tracer(tracer))
    .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
    .with(
      EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy(),
    )
    .init();

  Ok(())
}

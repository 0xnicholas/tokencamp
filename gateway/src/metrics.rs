use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct Metrics {
    pub requests_total: AtomicU64,
    pub requests_streaming: AtomicU64,
    pub errors_total: AtomicU64,
    pub rate_limited_total: AtomicU64,
    pub active_connections: AtomicU64,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            requests_total: AtomicU64::new(0),
            requests_streaming: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            rate_limited_total: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
        })
    }

    pub fn render(&self) -> String {
        format!(
            "# HELP tokencamp_requests_total Total requests\n\
             # TYPE tokencamp_requests_total counter\n\
             tokencamp_requests_total {}\n\
             # HELP tokencamp_requests_streaming_total Streaming requests\n\
             # TYPE tokencamp_requests_streaming_total counter\n\
             tokencamp_requests_streaming_total {}\n\
             # HELP tokencamp_errors_total Total errors\n\
             # TYPE tokencamp_errors_total counter\n\
             tokencamp_errors_total {}\n\
             # HELP tokencamp_rate_limited_total Rate limit hits\n\
             # TYPE tokencamp_rate_limited_total counter\n\
             tokencamp_rate_limited_total {}\n\
             # HELP tokencamp_active_connections Active SSE connections\n\
             # TYPE tokencamp_active_connections gauge\n\
             tokencamp_active_connections {}\n",
            self.requests_total.load(Ordering::Relaxed),
            self.requests_streaming.load(Ordering::Relaxed),
            self.errors_total.load(Ordering::Relaxed),
            self.rate_limited_total.load(Ordering::Relaxed),
            self.active_connections.load(Ordering::Relaxed),
        )
    }
}

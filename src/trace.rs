//! Opt-in tracing used to hunt down rendering stalls on CI.
//!
//! Enable with `SPACEFLIGHT_TRACE=1`. Each call prints the stage name and the
//! milliseconds since the tracer was created, so the last line of output tells
//! you exactly where a hang is.

use std::sync::OnceLock;
use std::time::Instant;

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("SPACEFLIGHT_TRACE")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false)
    })
}

pub fn start() -> Instant {
    static T0: OnceLock<Instant> = OnceLock::new();
    *T0.get_or_init(Instant::now)
}

pub fn trace(stage: &str) {
    if enabled() {
        let elapsed = start().elapsed().as_millis();
        eprintln!("[trace {:>7} ms] {}", elapsed, stage);
    }
}

//! PREDICT — deterministic short-horizon trend projection.
//!
//! DETECT answers *"what is wrong now?"*. PREDICT answers *"what is about to go
//! wrong, and how soon?"* — by fitting a line to recent history of a metric and
//! projecting when it will cross the same threshold DETECT fires on.
//!
//! This is deliberately *not* machine learning and *not* an LLM. It is ordinary
//! least-squares regression over a time window. The reason is trust: prediction
//! is where a naive implementation cries wolf, and a reliability layer that
//! cries wolf gets ignored — so every projection must survive a chain of gates
//! before it is allowed to become a `Prediction`.
//!
//! ## What it predicts, and the impact
//!
//! For each rising metric (memory %, swap %, hottest temperature), it estimates
//! *lead time* to the warning threshold and reports it with a probability and a
//! confidence. The impact is **actionable warning before the failure**: memory
//! climbing toward an OOM kill, swap filling toward stalls, temperature rising
//! toward thermal throttling — surfaced minutes early instead of at the crash.
//!
//! ## The gates (how it avoids being wrong)
//!
//! A projection is discarded unless *all* hold — this is the whole design:
//!
//! 1. **Enough history** — at least [`MIN_SAMPLES`] points spanning
//!    [`MIN_SPAN_SECS`]; too little data → no prediction (never a guess).
//! 2. **Real time variance** — if timestamps don't advance (`Σ(x-x̄)²≈0`),
//!    bail; protects against a stuck/backwards clock and divide-by-zero.
//! 3. **Rising meaningfully** — slope must exceed a small positive floor; a flat
//!    or *improving* metric predicts nothing (no phantom failures).
//! 4. **Consistent trend** — the fit's R² must clear [`R2_MIN`]; a noisy, jagged
//!    series is rejected rather than extrapolated (spike immunity).
//! 5. **Not already there** — if the current (fitted) value is at/over the
//!    threshold, that's DETECT's job now; PREDICT stays silent (no double alarm).
//! 6. **Within the horizon** — a crossing must be sooner than [`HORIZON_SECS`];
//!    projecting hours ahead from minutes of data is not credible, so it's cut.
//! 7. **Finite math** — every intermediate is checked for `NaN`/`inf`, and the
//!    reported probability is clamped to `[0,1]`.
//!
//! The probability is an honest heuristic (proximity-weighted, gated by fit),
//! **not** a calibrated statistical figure — it has not been validated against
//! real hardware failures. See *Honest status* in
//! `docs/RELIABILITY-ARKA-PULSE.md`.

use std::collections::VecDeque;
use std::collections::vec_deque::Iter;

/// Ring-buffer capacity for history (≈30 min at a 10 s cadence).
pub const HISTORY_CAP: usize = 180;
/// Fewest points before a projection is attempted.
const MIN_SAMPLES: usize = 6;
/// Shortest time span the points must cover, in seconds.
const MIN_SPAN_SECS: f64 = 30.0;
/// Minimum coefficient of determination for the fit to be trusted.
const R2_MIN: f64 = 0.6;
/// Farthest ahead a crossing may be projected, in seconds (30 min).
const HORIZON_SECS: f64 = 1800.0;

/// One projected failure, with the evidence behind it.
pub struct Prediction {
    pub domain: &'static str,
    pub summary: String,
    /// Heuristic likelihood in `[0,1]` — proximity-weighted, gated by fit.
    pub probability: f64,
    /// Fit quality in `[0,1]` (R²) — how consistent the trend is.
    pub confidence: f64,
    pub lead_time_secs: f64,
    pub evidence: String,
}

/// One point of retained history. Optional metrics are stored only when the
/// system actually exposes them (no swap configured, no thermal zone in a VM).
pub struct Sample {
    pub t: f64,
    pub mem_pct: f64,
    pub swap_pct: Option<f64>,
    pub temp_max: Option<f64>,
}

/// Bounded, time-ordered history of samples.
pub struct History {
    buf: VecDeque<Sample>,
    cap: usize,
}

impl History {
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1);
        History {
            buf: VecDeque::with_capacity(cap),
            cap,
        }
    }

    pub fn push(&mut self, s: Sample) {
        if self.buf.len() >= self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(s);
    }

    pub fn iter(&self) -> Iter<'_, Sample> {
        self.buf.iter()
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

/// Result of the least-squares fit over `(time, value)` points.
struct Trend {
    slope_per_sec: f64,
    r2: f64,
    /// Value predicted by the fit at the most recent timestamp (denoised).
    current_fit: f64,
    span_secs: f64,
}

/// Ordinary least-squares of `y` on `x` (time). Returns `None` on any
/// degenerate input (too few points, no time variance, non-finite result).
fn linreg(pts: &[(f64, f64)]) -> Option<Trend> {
    let n = pts.len();
    if n < MIN_SAMPLES {
        return None;
    }
    let nf = n as f64;
    let xbar = pts.iter().map(|p| p.0).sum::<f64>() / nf;
    let ybar = pts.iter().map(|p| p.1).sum::<f64>() / nf;

    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for &(x, y) in pts {
        let dx = x - xbar;
        let dy = y - ybar;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    if sxx <= f64::EPSILON {
        return None; // no variance in time — stuck/backwards clock
    }
    let slope = sxy / sxx;
    if !slope.is_finite() {
        return None;
    }
    // R² is undefined for a perfectly flat series; treat as "no trend info".
    let r2 = if syy <= f64::EPSILON {
        0.0
    } else {
        ((sxy * sxy) / (sxx * syy)).clamp(0.0, 1.0)
    };

    let x_first = pts.first().unwrap().0;
    let x_last = pts.last().unwrap().0;
    let current_fit = ybar + slope * (x_last - xbar);
    if !current_fit.is_finite() || !r2.is_finite() {
        return None;
    }

    Some(Trend {
        slope_per_sec: slope,
        r2,
        current_fit,
        span_secs: x_last - x_first,
    })
}

/// Project a rising metric toward `warn`, returning a `Prediction` only if every
/// gate in the module docs passes.
fn predict_rising(
    what: &str,
    domain: &'static str,
    unit: &str,
    series: &[(f64, f64)],
    warn: f64,
    min_slope: f64,
) -> Option<Prediction> {
    let tr = linreg(series)?;

    if tr.span_secs < MIN_SPAN_SECS {
        return None; // gate 1: not enough time covered
    }
    if tr.slope_per_sec <= min_slope {
        return None; // gate 3: flat or improving — nothing coming
    }
    if tr.r2 < R2_MIN {
        return None; // gate 4: too noisy to trust
    }
    let current = tr.current_fit;
    if current >= warn {
        return None; // gate 5: already in DETECT's territory
    }

    let lead = (warn - current) / tr.slope_per_sec;
    if !lead.is_finite() || lead <= 0.0 || lead > HORIZON_SECS {
        return None; // gate 6/7: non-finite, past, or beyond the horizon
    }

    let proximity = 1.0 - lead / HORIZON_SECS;
    let probability = (0.4 + 0.6 * proximity).clamp(0.0, 1.0);
    let confidence = tr.r2;
    let mins = lead / 60.0;

    Some(Prediction {
        domain,
        summary: format!("{what} approaching {warn:.0}{unit} — likely within ~{mins:.0} min"),
        probability,
        confidence,
        lead_time_secs: lead,
        evidence: format!(
            "now ~{current:.1}{unit}, rising {:.3}{unit}/s over {:.0}s (r²={:.2})",
            tr.slope_per_sec, tr.span_secs, tr.r2
        ),
    })
}

/// Run every projection over the retained history.
pub fn evaluate(h: &History) -> Vec<Prediction> {
    use crate::detect::thresh;
    let mut out = Vec::new();

    let mem: Vec<(f64, f64)> = h.iter().map(|s| (s.t, s.mem_pct)).collect();
    if let Some(p) = predict_rising("Memory use", "memory", "%", &mem, thresh::MEM_WARN_PCT, 0.01) {
        out.push(p);
    }

    let swap: Vec<(f64, f64)> = h.iter().filter_map(|s| s.swap_pct.map(|v| (s.t, v))).collect();
    if let Some(p) = predict_rising("Swap use", "memory", "%", &swap, thresh::SWAP_WARN_PCT, 0.01) {
        out.push(p);
    }

    let temp: Vec<(f64, f64)> = h.iter().filter_map(|s| s.temp_max.map(|v| (s.t, v))).collect();
    if let Some(p) = predict_rising("Temperature", "thermal", "°C", &temp, thresh::TEMP_WARN_C, 0.01)
    {
        out.push(p);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a series starting at `start`, rising by `step` every `dt` seconds.
    fn ramp(start: f64, step: f64, dt: f64, n: usize) -> Vec<(f64, f64)> {
        (0..n).map(|i| (i as f64 * dt, start + step * i as f64)).collect()
    }

    #[test]
    fn rising_series_predicts_with_lead_time() {
        // 50% climbing 2%/sample every 10s → 0.2%/s, crosses 90% well within horizon.
        let s = ramp(50.0, 2.0, 10.0, 13);
        let p = predict_rising("Memory", "memory", "%", &s, 90.0, 0.01)
            .expect("a clean upward ramp toward the threshold must predict");
        assert!(p.lead_time_secs > 0.0 && p.lead_time_secs <= HORIZON_SECS);
        assert!(p.probability > 0.0 && p.probability <= 1.0);
        assert!(p.confidence >= R2_MIN);
    }

    #[test]
    fn flat_series_predicts_nothing() {
        let s: Vec<(f64, f64)> = (0..12).map(|i| (i as f64 * 10.0, 50.0)).collect();
        assert!(predict_rising("Memory", "memory", "%", &s, 90.0, 0.01).is_none());
    }

    #[test]
    fn improving_series_predicts_nothing() {
        // Falling toward safety must never be projected as a failure.
        let s = ramp(74.0, -2.0, 10.0, 12);
        assert!(predict_rising("Memory", "memory", "%", &s, 90.0, 0.01).is_none());
    }

    #[test]
    fn noisy_series_is_rejected() {
        // Alternating spikes: ~zero net slope, low R² → no prediction.
        let s: Vec<(f64, f64)> = (0..12)
            .map(|i| (i as f64 * 10.0, if i % 2 == 0 { 50.0 } else { 88.0 }))
            .collect();
        assert!(predict_rising("Memory", "memory", "%", &s, 90.0, 0.01).is_none());
    }

    #[test]
    fn too_few_samples_predicts_nothing() {
        let s = ramp(50.0, 10.0, 10.0, 3);
        assert!(predict_rising("Memory", "memory", "%", &s, 90.0, 0.01).is_none());
    }

    #[test]
    fn already_over_threshold_is_detects_job() {
        // Sitting at ~92–95%, above the 90% warn line → PREDICT stays silent.
        let s = ramp(92.0, 0.5, 10.0, 7);
        assert!(predict_rising("Memory", "memory", "%", &s, 90.0, 0.01).is_none());
    }

    #[test]
    fn crossing_beyond_horizon_is_cut() {
        // Rising, consistent, but so slowly the crossing is >30 min away.
        // start ~47.75, +0.015%/s → to 90% is ~2800s > HORIZON.
        let s = ramp(47.75, 0.75, 50.0, 7);
        assert!(predict_rising("Memory", "memory", "%", &s, 90.0, 0.01).is_none());
    }

    #[test]
    fn history_ring_buffer_bounds_length() {
        let mut h = History::new(4);
        for i in 0..10 {
            h.push(Sample {
                t: i as f64,
                mem_pct: 10.0,
                swap_pct: None,
                temp_max: None,
            });
        }
        assert_eq!(h.len(), 4);
        // Oldest three dropped: first retained timestamp is 6.0.
        assert_eq!(h.iter().next().unwrap().t, 6.0);
    }
}

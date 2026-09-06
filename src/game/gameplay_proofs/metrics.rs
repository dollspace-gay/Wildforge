//! Raw native update timings for this bounded functional scenario.

use std::time::Instant;

use super::Game;

#[derive(Default)]
pub(super) struct Samples {
    updates_ms: Vec<f64>,
    pending_chunks_peak: usize,
}

impl Samples {
    pub(super) fn update(&mut self, game: &mut Game) {
        let started = Instant::now();
        game.update();
        self.updates_ms
            .push(started.elapsed().as_secs_f64() * 1000.0);
        self.pending_chunks_peak = self.pending_chunks_peak.max(game.chunk_work_pending());
    }

    pub(super) fn report(mut self) -> serde_json::Value {
        assert!(!self.updates_ms.is_empty());
        self.updates_ms.sort_by(f64::total_cmp);
        let percentile = |p: f64| {
            self.updates_ms[((self.updates_ms.len() as f64 * p).ceil() as usize - 1)
                .min(self.updates_ms.len() - 1)]
        };
        let status = std::fs::read_to_string("/proc/self/status").expect("Linux process status");
        let rss_kib: u64 = status
            .lines()
            .find_map(|line| line.strip_prefix("VmHWM:"))
            .and_then(|line| line.split_whitespace().next())
            .expect("Linux peak resident memory")
            .parse()
            .unwrap();
        serde_json::json!({
            "updates": self.updates_ms.len(),
            "update_ms_p50": percentile(0.50),
            "update_ms_p95": percentile(0.95),
            "update_ms_p99": percentile(0.99),
            "pending_initial_chunks_peak": self.pending_chunks_peak,
            "process_peak_rss_kib": rss_kib,
        })
    }
}

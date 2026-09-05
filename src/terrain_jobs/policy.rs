//! Existing caller-specific worker counts; adoption budgets stay with callers.

/// Keeps interactive and dedicated preparation capacity explicit.
#[derive(Clone, Copy, Debug)]
pub(crate) enum WorkerPolicy {
    Interactive,
    Dedicated,
}

impl WorkerPolicy {
    pub(super) fn count(self, parallelism: Option<usize>) -> usize {
        match self {
            Self::Interactive => parallelism.map(|n| (n / 2).clamp(1, 8)).unwrap_or(2),
            Self::Dedicated => parallelism
                .map(|n| n.saturating_sub(2).clamp(2, 4))
                .unwrap_or(2),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkerPolicy;

    #[test]
    fn existing_worker_budgets_are_preserved() {
        for (cores, interactive, dedicated) in [
            (None, 2, 2),
            (Some(1), 1, 2),
            (Some(4), 2, 2),
            (Some(8), 4, 4),
            (Some(32), 8, 4),
        ] {
            assert_eq!(WorkerPolicy::Interactive.count(cores), interactive);
            assert_eq!(WorkerPolicy::Dedicated.count(cores), dedicated);
        }
    }
}

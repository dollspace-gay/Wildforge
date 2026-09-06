//! Save reports and compatibility adapters.

use std::{fmt, path::PathBuf};

/// One persistence component that did not reach durable storage.
#[derive(Debug)]
pub struct SaveFailure {
    pub component: String,
    pub path: PathBuf,
    pub error: std::io::Error,
}

impl SaveFailure {
    pub(super) fn new(component: impl Into<String>, path: PathBuf, error: std::io::Error) -> Self {
        Self {
            component: component.into(),
            path,
            error,
        }
    }
}

impl fmt::Display for SaveFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}): {}",
            self.component,
            self.path.display(),
            self.error
        )
    }
}

/// Complete result of one world-save attempt.
///
/// Independent components continue after a failure so operators get one
/// useful report instead of discovering errors one five-minute retry at a
/// time. Dirty chunks are cleared only for writes that landed.
#[derive(Debug, Default)]
#[must_use = "world save failures must be reported or handled"]
pub struct SaveReport {
    pub chunks_saved: usize,
    pub failures: Vec<SaveFailure>,
}

impl SaveReport {
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn summary(&self) -> String {
        if self.is_ok() {
            return format!("{} dirty chunks written", self.chunks_saved);
        }
        let shown = self
            .failures
            .iter()
            .take(3)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        let more = self.failures.len().saturating_sub(3);
        if more == 0 {
            shown
        } else {
            format!("{shown}; and {more} more")
        }
    }

    pub(super) fn record(
        &mut self,
        component: impl Into<String>,
        path: PathBuf,
        result: std::io::Result<()>,
    ) -> bool {
        match result {
            Ok(()) => true,
            Err(error) => {
                self.failures.push(SaveFailure::new(component, path, error));
                false
            }
        }
    }

    pub(super) fn extend(&mut self, failures: impl IntoIterator<Item = SaveFailure>) {
        self.failures.extend(failures);
    }
}

/// Result of one residency sweep.
#[derive(Debug, Default)]
#[must_use = "residency save failures must be reported or handled"]
pub struct ResidencyReport {
    pub released: usize,
    pub retained_dirty: usize,
    pub failures: Vec<SaveFailure>,
}

impl ResidencyReport {
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn summary(&self) -> String {
        if self.is_ok() {
            return format!("released {} chunks", self.released);
        }
        let shown = self
            .failures
            .iter()
            .take(3)
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        let more = self.failures.len().saturating_sub(3);
        let suffix = if more == 0 {
            String::new()
        } else {
            format!("; and {more} more")
        };
        format!(
            "released {}, retained {} dirty: {shown}{suffix}",
            self.released, self.retained_dirty
        )
    }
}

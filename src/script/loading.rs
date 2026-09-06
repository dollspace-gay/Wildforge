//! Compile privately, then publish an entire script set without replacing host state.

use std::fmt;
use std::io;
use std::path::PathBuf;

use super::{ScriptHost, ScriptMod};

/// Compiled against the same host engine that will execute these ASTs.
#[must_use = "prepared scripts have no effect until installed"]
pub struct PreparedScripts {
    mods: Vec<ScriptMod>,
}

#[derive(Debug)]
pub struct ScriptErrors {
    diagnostics: Vec<String>,
}

impl ScriptErrors {
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    fn check(mods: &[ScriptMod]) -> Result<(), Self> {
        let diagnostics: Vec<_> = mods
            .iter()
            .filter_map(|script| script.error.clone())
            .collect();
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(Self { diagnostics })
        }
    }
}

impl fmt::Display for ScriptErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("script compilation failed:")?;
        for diagnostic in &self.diagnostics {
            write!(formatter, "\n{diagnostic}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ScriptErrors {}

impl ScriptHost {
    /// Compile every provider without executing scripts or changing live ASTs.
    pub fn prepare_mods(
        &self,
        mods: &[(String, PathBuf)],
    ) -> Result<PreparedScripts, ScriptErrors> {
        let mods = self.compile_mods(mods, true);
        ScriptErrors::check(&mods)?;
        Ok(PreparedScripts { mods })
    }

    pub fn validate_loaded(&self) -> Result<(), ScriptErrors> {
        ScriptErrors::check(&self.mods)
    }

    /// Retain the host's engine, KV store, and pending command queue.
    pub fn install_prepared(&mut self, prepared: PreparedScripts) {
        self.mods = prepared.mods;
    }

    /// Compile `main.rhai` for each mod dir. On error, keep that mod's previous
    /// AST when present. Atomic content reload uses `prepare_mods` instead.
    pub fn load_mods(&mut self, mods: &[(String, PathBuf)]) {
        let mut next = self.compile_mods(mods, false);
        for script in &mut next {
            if script.error.is_some() {
                script.ast = self
                    .mods
                    .iter_mut()
                    .find(|old| old.id == script.id)
                    .and_then(|old| old.ast.take());
            }
        }
        self.mods = next;
    }

    fn compile_mods(&self, mods: &[(String, PathBuf)], require_present: bool) -> Vec<ScriptMod> {
        let mut next = Vec::new();
        for (id, dir) in mods {
            let source = match std::fs::read_to_string(dir.join("main.rhai")) {
                Ok(source) => Ok(source),
                Err(error) if error.kind() == io::ErrorKind::NotFound && !require_present => {
                    continue;
                }
                Err(error) => Err(error.to_string()),
            };
            match source.and_then(|source| {
                self.engine
                    .compile(&source)
                    .map_err(|error| error.to_string())
            }) {
                Ok(ast) => next.push(ScriptMod {
                    id: id.clone(),
                    ast: Some(ast),
                    error: None,
                }),
                Err(error) => next.push(ScriptMod {
                    id: id.clone(),
                    ast: None,
                    error: Some(format!("{id}/main.rhai: {error}")),
                }),
            }
        }
        next
    }
}

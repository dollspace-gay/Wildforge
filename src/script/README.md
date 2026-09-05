<!-- wildforge:guide -->

# Script compilation and publication

`loading.rs` compiles complete candidate script sets using the current host
engine, without executing scripts or replacing live ASTs. A successful prepared
set installs once; failed preparation retains the running set. The existing
`load_mods` API keeps per-mod previous-AST fallback for its existing callers.
The parent `script.rs` owns sandbox bindings, dispatch, queued commands, and KV
state. Preparing/installing scripts must preserve those owners.

Read [AGENTS.md](AGENTS.md). Final checks cover good/bad/missing scripts, all-or-
nothing hot reload, prior-AST fallback, retained KV state, and command order.

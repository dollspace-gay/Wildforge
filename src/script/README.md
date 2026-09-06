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

Dispatch reads use a borrowed WorldView. Both graphical runtime variants supply a bounded view; the World wrapper
serves existing test scenarios. The scoped thread-local
read bridge retains its borrow through synchronous dispatch and restores the
previous bridge on unwind. Host functions receive only bounded read queries.

`context_tests.rs` checks nested dispatch, panic unwinding and thread isolation
for the scoped read bridge. Run these cases under Miri when that unsafe bridge
changes, in addition to the normal script and reload scenarios.

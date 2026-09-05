# Maintainability report

Run from any directory with Python 3.10+ and Git:

```sh
python3 tools/check_maintainability.py
python3 tools/check_maintainability.py --format json
python3 tools/check_maintainability.py --base <git-revision>
python3 tools/check_maintainability.py --json-output /tmp/maintainability.json
python3 -m unittest discover -s tools/tests -v
```

The script defaults to its repository root. Use `--root` to scan another Git
checkout. It measures tracked and unignored new `.rs`, `.py`, `.wgsl`, `.rhai`,
and `.sh` regular files. Git-ignored content, deleted files, and symlinks are
not scanned. No source changes or baseline updates are performed by a scan.

The default physical-line thresholds are **over 400: warning**, **over 500:
review**. Comments, blank lines, embedded data, and inline tests count. Both
levels are advisory. Keep useful documentation and formatting; split files
where responsibility and ownership justify a module boundary.

The clone scan covers Rust, Python, and WGSL. It preserves identifiers and
literal values, removes comments/formatting, and reports exact repeated spans
of at least 100 tokens on at least 12 token-bearing lines per occurrence.
Python uses its standard tokenizer. Rust/WGSL use a lexical scanner that
recognizes nested comments and Rust string forms; it is not a compiler parser.
Matches are verified token by token after winnowing rolling fingerprints.

Reports nominate code for review. They do not prove semantic duplication or
find renamed/rewritten equivalents. Generated-looking code and tests are not
silently exempted. Highly repetitive fingerprints occurring more than 64 times
are skipped to bound candidate comparison cost; the skipped count is visible.
Nested clone groups can overlap, so their sizes are not an additive debt metric.

Text output shows the largest 20 entries in each section. `--limit` changes that
presentation limit; JSON includes all findings. Clone IDs hash language plus
exact matched tokens, so moving an unchanged span down a file does not change
its ID. Extending or modifying the matched span can change it.
JSON records the analyzer and Python versions. Compare reports on the same
versions: Python's tokenizer can change between interpreter releases.

Use `--warn-lines`, `--review-lines`, `--min-tokens`, and `--min-lines` for local
experiments. CI uses the defaults. A report with findings exits 0; invalid
arguments or incomplete scans exit nonzero. There is no suppression, semantic
analysis or merge-blocking debt baseline in the size/clone checker. Separate
architecture and compiler-backed reports are described below.

The migration and later checks are specified in
[the refactor plan](../../.design/maintainability-refactor.md).

## Compare a change against its base

`--base` resolves the supplied ref to one immutable commit before reading any
source. The analyzer reads committed Git blobs directly, scans the current
working tree with the same rules, and adds a complete `comparison` object to
JSON plus a concise text summary. It does not check out or execute base code.

File findings report new, grown, reduced, and removed size debt. Git-detected
renames retain their previous path; an unchanged renamed file is identified
explicitly. Untracked moves may appear as a removed file and a new file until
staged, because Git has no tracked rename to compare yet. Clone families retain
their exact-token IDs through moves and line shifts; changed occurrence counts
are reported as grown/reduced, while changed tokens can appear as a removed
family and a new family. These are finding changes, not semantic judgments.

The comparison includes only files that exceed the soft warning on either
side and clone families whose counts or locations changed. JSON retains both
sets of clone locations. A broken/unknown base fails the scan, and differing
analyzer versions or thresholds cannot be compared as equivalent measurements.
CI compares pull requests against their pinned base SHA; push runs print the
current inventory. Neither path makes size or duplication findings a merge gate.

## Written dependency boundaries

`python3 tools/check_architecture.py` loads the explicit allowlist in
[architecture-boundaries.json](../../.design/architecture-boundaries.json).
It covers worker ownership, terrain preparation, generation, atlas/content,
guest protocol, player operations, simulation, and renderer modules. Each rule
records its responsibility, source selectors, allowed paths, and test additions.
An empty selector or malformed contract fails; it cannot claim an empty clean
boundary. Current World dependencies in guest interpretation remain an explicit
migration seam, not completed authority separation.

The token scanner handles nested use trees, aliases, written qualified paths,
inline modules, and re-export/glob paths. Strings/comments cannot introduce
edges. Root namespace globs are findings. Block-local and conditional imports
form a conservative union; review possible over-reporting. External module
names follow the current source layout, with the existing multiplayer/mp path
mapping. Nonstandard path attributes and test names need review when changing
that layout. Cyclic resolution notes remain visible in the report.

This is a written dependency check, not Rust type resolution, macro expansion,
a call graph, or proof of mutation effects. Macro bodies are inspected as written
tokens; dependencies created by expansion can remain invisible. Renderer rules
exclude explicit authoritative types and operations; they cannot prove the
behavior of an inferred value or a permitted wrapper. Final validation includes
violating/alias/glob fixtures and a reviewed noise inventory.

Use `--format json` or `--json-output PATH` for every finding and edge. Findings
are advisory in CI; `--strict` explicitly exits 1 for policy violations. Scanner
or configuration failure exits 2 in either mode. Size limits remain soft.

## Compiler-backed function reports

`python3 tools/report_rust_functions.py` invokes Clippy for all current-platform
Cargo targets. It force-enables `too_many_lines` and `cognitive_complexity` as
warnings, captures JSON diagnostics, and records the actual Clippy version,
command, configuration, completion, and compiler errors. `clippy.toml` sets the
advisory thresholds to 100 function lines and 25 cognitive complexity. Clippy's
line definition differs from physical file size; cognitive complexity is not
cyclomatic complexity. There is no brace/keyword estimate in this tool.

The report contains above-threshold findings, not a measurement of every function.
Cfg-disabled platforms/features are not compiler coverage. Duplicate library/test
diagnostics are consolidated only when all finding fields match. Force-warn
keeps these findings visible under source allow attributes. Existing strict
Clippy runs independently; this report does not weaken its warning gate.

Use `--format json`, `--json-output PATH`, and a positive `--timeout SECONDS` as
needed. Findings exit 0 only after a successful complete compiler run. Build,
compiler, parse, timeout, or toolchain failure exits 2. On POSIX, interruption
terminates and reaps the compiler process group. No source or tests are executed
by the report, although Clippy performs normal compilation/build-script work.

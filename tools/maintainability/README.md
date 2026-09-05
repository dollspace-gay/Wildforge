# Maintainability report

Run from any directory with Python 3.10+ and Git:

```sh
python3 tools/check_maintainability.py
python3 tools/check_maintainability.py --format json
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
analysis, dependency-rule checker, or merge-blocking debt baseline in this version.

The migration and later checks are specified in
[the refactor plan](../../.design/maintainability-refactor.md).

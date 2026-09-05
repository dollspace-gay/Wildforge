<!-- wildforge:guide -->

# Native visual campaigns

This package drives the full strata, cracked-geode, performance, and motion
campaign. It copies immutable input saves for individual runs and retains failed
attempts. Existing metric implementations remain in the three `verify_*` tools;
this package supplies actual campaign identities and routes their artifacts.

Run `python3 tools/run_visual_campaign.py --help` from the repository root.
The baseline and candidate executables must come from clean, recorded commits.
Captures require native GPU rendering and a visible X11 session. Keep raw frames,
binaries, and saves in ignored working storage; publish only verified metadata
and reports into a new directory selected by `screenshots/current-campaign.toml`.

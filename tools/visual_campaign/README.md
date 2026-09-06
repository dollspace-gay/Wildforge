<!-- wildforge:guide -->

# Native visual campaigns

This package drives the full strata, cracked-geode, performance, and motion
campaign. It copies immutable input saves for individual runs and retains failed
attempts. Existing metric implementations remain in the three `verify_*` tools;
this package supplies actual campaign identities and routes their artifacts.

Run `python3 tools/run_visual_campaign.py --help` from the repository root.
The baseline and candidate executables must come from clean, recorded commits.
Still captures require native GPU rendering and an X11 session. Motion recording
requires `Xvfb`, `xdotool`, and `libX11`: an owned isolated display prevents desktop
shortcuts from intercepting F2. The game must report a discrete Vulkan adapter;
the display server does not replace hardware rendering. The runner waits for
the game's initial-upload settlement event, records 28 F2 frames per walk with
short native WASD steps, verifies input traces, and requests normal window
closure through `WM_DELETE_WINDOW`. Review the frames and populate the motion
review file before qualifying; successful input delivery alone is not a pass.

Keep raw frames,
binaries, and saves in ignored working storage; publish only verified metadata
and reports into a new directory selected by `screenshots/current-campaign.toml`.

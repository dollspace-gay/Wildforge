<!-- wildforge:guide -->

# Visual qualification records

Capture metadata, manifests, timing observations, and tracked diagnostic data describe reproducible visual scenarios. Generated PNG captures may be ignored.

`current-campaign.toml` selects the complete campaign checked by CI. See the
[campaign workflow](../docs/visual-qualification-campaign.md) to rebuild all
native captures and motion evidence. Older files retain their original dated
provenance.

Start with `closeout-geode-aperture-proof.capture.toml`, `closeout-geode-cracked-hero.capture.toml`, `closeout-geode-performance-opened-a.capture.toml`, `closeout-geode-performance-opened-b.capture.toml`, `closeout-geode-performance-opened-c.capture.toml`, `closeout-geode-performance-opened-d.capture.toml`, `closeout-geode-performance-opened-e.capture.toml`, `closeout-geode-performance-sealed-a.capture.toml`. See the [repository overview](../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

A new renderer result needs a new real capture and matching provenance. Do not edit old timing/source identities to manufacture a pass.

## Focused checks

```sh
python3 tools/verify_visual_polish.py --help
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.

<!-- wildforge:guide -->

# Working in screen composition

Read [README.md](README.md), [the game guide](../README.md), and the root AGENTS.
Keep menu-only, HUD, panel, and tooltip ordering explicit. Preserve layout values,
widget hit regions, text, and guest observation behavior during structural moves.
Do not introduce simulation writes or new broad runtime capabilities into drawing.
Prefer the existing shared widgets and inventory geometry to copied slot rules.
Review source files above 500 lines and add guidance for any new directory.
Record UI/native GPU validation and any missing evidence in the progress record.

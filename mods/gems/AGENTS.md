<!-- wildforge:guide -->

# Working in mods/gems

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

mod.toml declares the gems namespace and dependency on base. Its definitions demonstrate gemstone world generation, items, recipes, and textures using the normal mod loader.

Keep content IDs, finite material vectors, and dependency order stable. New guide files begin with the wildforge:guide HTML comment so documentation cannot alter the mod or signed genesis identity. Test content changes through registry and world-generation scenarios.

Run the focused registry and content-identity tests from the root, then the applicable full gates. Provide both local guides for new maintained subdirectories.

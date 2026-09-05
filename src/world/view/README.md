<!-- wildforge:guide -->

# Scene observations

WorldView privately selects an authoritative World or an independent ReplicaWorld
and exposes observations only. It implements TerrainRead, SceneRead, and BlockRead;
it has no mutation, save, generation, or raw-owner accessor. physics and meshing
continue to require only the narrower terrain contract.

The child modules separate environmental, entity/structure, machine, and item
queries. Shared calendar, machine-recognition, and item-presentation functions
retain the same rules for both sources. Host-only ledgers and structures that
were absent from the previous guest representation stay absent from the replica.

Read [AGENTS.md](AGENTS.md). Final checks cover read parity, missing/partial host
data, item remapping, shape matching, selection, and real graphical UI/rendering
against both local and remote scenes.

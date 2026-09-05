<!-- wildforge:guide -->

# Working in agent scenarios

Read [README.md](README.md) and [the parent guidance](../AGENTS.md).

Keep tests on the public guest and transport paths. Reuse the stage fixture;
do not introduce privileged client actions. Use ordered message barriers and
bounded condition waits rather than fixed sleeps as evidence of delivery.
Join any paused host thread and keep these tests in the serial agent lane.

# Progress toward RA2NE 1.0

As of 2026-10-04: **approximately 25% complete; approximately 75% remaining**.
This is an engineering estimate of feature coverage, not a measured acceptance
score, elapsed-time percentage or prediction of remaining work hours. Small
rule additions do not automatically increase the displayed percentage.

The target is a modern engine that actually plays with user-owned original RA2/YR
resources, original maps and ordinary INI mods. Ares/Phobos complete compatibility
is a later goal. Synthetic tests alone cannot satisfy this target.

Implemented foundations include deterministic simulation, shared path planning,
basic isometric rendering/input, synthetic combat/economy/production, engine
saves/replays, TCP input synchronization, resource format readers and a partial
INI-to-game adapter. Current original-content gameplay acceptance is pending.

Major remaining work:
- Original terrain/passability, occupancy, avoidance and movement semantics.
- Complete units, projectiles, splash/immunities, construction and triggers.
- Original resource rendering, voxels, animations, theaters, shadows and audio.
- Full faction/technology/production rules, skirmish AI and playable multiplayer UI.
- End-to-end acceptance with real maps/mods and cross-platform determinism.

Verified batches and their specific limitations are recorded in STATUS.md.
Reassess the overall estimate at substantial end-to-end milestones rather than
counting commits or unit tests as progress points.

# Progress toward RA2NE 1.0

As of 2026-10-05: **approximately 25% complete; approximately 75% remaining**.
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

Actual package parsing has now been checked for identified maps, SHP frames and TMP tiles; see REAL_RESOURCE_VALIDATION.md. This does not change the original gameplay acceptance status.

Stock directory terrain viewing now loads samples from all six theaters, handles
clear markers and composites extra graphics with matching ISO palettes. Window
screenshots were inspected. Full map rendering and original gameplay acceptance
are still pending; the overall coverage estimate remains unchanged.

Static original overlays now render with stock rules/art aliases, SHP frames and
theater palettes. Six theater samples resolve all overlay references. A strict
371-map graphics check passes 360 maps and reports 11 remaining failures involving
terrain resources/subtiles and overlay frame references. These are documented
acceptance gaps; original gameplay and the 1.0 estimate remain unchanged.

Placed trees, lamps and signs now render as static original scenery. All 165,818
scenery references across 371 maps resolve, with inspected release screenshots.
This advances original-resource viewing; scenery collision, animation and full
original gameplay remain pending. The overall estimate remains approximately 25%.

Static original infantry now render with standing sequence frames, facings,
subcells and elevation. All 4,011 placed infantry references across 371 maps
resolve. This does not yet include owner remap, shadows, action animation, VXL
vehicles/aircraft or original combat. Overall estimated coverage remains 25%.

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

VXL/HVA geometry and pose readers now parse 221 models / 221 pose files from the
real package, with zero format failures. Selected stock resources provide 104
model/pose bindings. The next rendering work still needs rasterization, normal
lighting, turrets/barrels, remap and shadows; model parsing is not full vehicle
support. Overall coverage remains approximately 25%.

Experimental unlit VXL vehicles/aircraft now draw in the map viewer, with optional
body/turret/barrel composition. A native resource showcase animates rotating
vehicles and original infantry Walk frames. This is a rendering milestone;
normal lighting, remap, shadows, attachment calibration and original gameplay
integration remain pending. Overall coverage remains approximately 25%.

Experimental owner-color previews now use rules/map HSV definitions for vehicles
and infantry while preserving neutral palettes and art remap flags. Cache keys
separate owner colors. Stock gradient/lighting calibration remains pending;
overall estimated coverage remains approximately 25%.

The strict 371-map graphics acceptance check is now green with no clamping or
substitution. REA on the original engine showed that every previously reported
reference draws nothing there (missing terrain files, IDs beyond the theater
catalogue, empty tile blocks, frames past the SHP frame table), so the check
freezes those tolerated no-image outcomes in a reviewed expectation table and
fails on anything unexpected. This closes the documented P0 acceptance gaps for
these maps; original gameplay, animation and the 1.0 end-to-end target are still
pending, so the overall estimate remains approximately 25%.

The runtime now loads the stock package the way gamemd.exe does. REA recovered
the full archive registration order (language packs first, then EXPANDMD99..00
down to the map/multi/theme packs, the scenario LOAD/NEUTRAL packs, and the
selected theater group last), the first-registered-wins rule (the 1.001
expandmd01.mix rulesmd.ini must beat the localmd.mix copy) and the
archives-before-disk lookup, so loose `.mix` files only matter when the
registration list names them and a loose `rulesmd.ini` is ignored while an
archived copy exists. Loose game-directory files remain the lowest layer, and
the gaps (movie/per-side/WDT packs, disk `RULEMD*.INI` merge layers) are
documented instead of guessed at. The 371-map acceptance re-run produced
byte-identical totals with 0 failing maps, confirming the wider mount set
changes no resolution outcome.

VXL previews now reproduce the original engine's normal lighting. REA recovered
the four generation normal tables, the fixed startup light direction, the
per-model shade table (`round(16·(max(0,n·L) + max(0,n·H/(3−2·n·H))))`) and the
`voxels.vpl` section lookup the draw loop performs, so the previews shade with
the shipped tables instead of the raw voxel colours. Stock acceptance shades ten
models (93,247 voxels, generations 2 and 4) and requires every drawn pixel to
match the section derived from the model's own voxels. Two engine details remain
unreproduced (the translated light transform and the out-of-buffer section read)
and are documented as gaps; owner remap, shadows, attachment calibration and
original gameplay integration remain pending. Overall coverage remains
approximately 25%.

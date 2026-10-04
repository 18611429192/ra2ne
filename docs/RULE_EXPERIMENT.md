# Experimental INI gameplay bridge

Layered INI type definitions can now drive a two-army test scenario. This is an
explicit experiment on the synthetic navigation grid, not an original-map game.
No original resources are included. Run the redistributable fixture:

```sh
cargo run --release -p ra2ne-runtime -- --rules-experiment=fixtures/rules-experiment.ini --rule-unit=TESTTANK --rule-speed=5:1 --rule-rof=1:1 --rule-build-ticks=90 --units=512 --autoplay --headless-ticks=900
```

Omit `--headless-ticks` for the interactive window. Append
`--save-game=experiment.ra2nsave` to save the imported definitions and state;
restore with `--load-game=experiment.ra2nsave`. Loading the engine save does not
require the source INI. Output refuses to overwrite existing files.

`--rules-overlay=PATH` may repeat; each later layer replaces individual keys.
`--encoding=gbk` or `--encoding=windows1252` applies to all rule layers.
`--rule-unit=ID` selects the registered mobile type for both armies. Registry
order remains stable, and type IDs match without ASCII case sensitivity.

| Rule | Current engine treatment |
| --- | --- |
| Strength, Cost | Imported exactly; positive health, nonnegative cost required |
| Speed | Each mobile speed requires explicit `--rule-speed=original:engine`; engine accepts 0–16 |
| Primary Damage | Nonnegative direct damage; healing weapons rejected |
| Primary Range | Positive whole cells, at most 1024; fractional ranges rejected |
| Primary ROF | `ceil(ROF * numerator / denominator)`, minimum one Tick, using `--rule-rof=N:D` |
| Power | Signed integer applied to the engine power balance |
| Harvester | Checked boolean; synthetic harvesting behavior with warning |
| BuildingTypes | Stationary definitions; no original construction/footprint behavior |
| AircraftTypes | Import fails; air movement is unavailable |
| Factory | Preserved with diagnostic; production disabled to avoid ignoring categories |
| Secondary, Armor, Prerequisite, Owner, Sight | Preserved with explicit omitted-behavior diagnostics |
| Warhead, Projectile | Direct uniform damage only, explicit diagnostic |
| Other properties | Retained by asset frontend, source/line diagnostics |

Build duration must be supplied explicitly with `--rule-build-ticks=N`; it is
not derived from original Cost/BuildSpeed semantics. Speed and ROF calibration
are caller choices, not verified RA2 timing conversions. Import does not clamp
invalid values, invent missing weapon definitions, or silently round ranges.

The game library exposes `rule_import::import` independently of the runtime.
The returned diagnostics must be shown to users before using experimental rules.
Runtime prints all diagnostics to stderr; the graphical scene keeps the first
100. Armor multipliers, projectiles, secondary selection, prerequisites, faction
ownership, terrain movement classes and original production remain necessary
for ordinary original-game and MOD compatibility. Importing data is not proof
that those behaviors are compatible.

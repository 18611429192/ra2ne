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
| Armor | Eleven standard armor slots; unknown/custom armor rejected |
| Warhead Verses | Exactly eleven percentages; exact thousandths of a percent; target armor scales direct damage |
| Secondary, Prerequisite, Owner, Sight | Preserved with explicit omitted-behavior diagnostics |
| Projectile and other warhead properties | Immediate direct hits; other properties retained with diagnostics |
| Other properties | Retained by asset frontend, source/line diagnostics |

Build duration must be supplied explicitly with `--rule-build-ticks=N`; it is
not derived from original Cost/BuildSpeed semantics. Speed and ROF calibration
are caller choices, not verified RA2 timing conversions. Import does not clamp
invalid values, invent missing weapon definitions, or silently round ranges.

The game library exposes `rule_import::import` independently of the runtime.
The returned diagnostics must be shown to users before using experimental rules.
Runtime prints all diagnostics to stderr; the graphical scene keeps the first
100. Projectiles, secondary selection, prerequisites, faction
ownership, terrain movement classes and original production remain necessary
for ordinary original-game and MOD compatibility. Importing data is not proof
that those behaviors are compatible.

Armor slot order is `none, flak, plate, light, medium, heavy, wood, steel,
concrete, special_1, special_2`. A referenced warhead must provide `Verses`;
missing definitions fail. Weapons without a warhead retain the synthetic 100%
multiplier. INI overlays also replace warhead Verses. Custom ArmorTypes and
Versus.* behavior are not implemented.

Direct damage uses `floor(Damage * multiplier / 100000)` and saturates at
`u32::MAX`; 100% is encoded as 100000. Zero multiplier disallows explicit attack
and automatic selection. Exact 1% and 2% values permit explicit attacks but
disable automatic acquisition. Retaliation is not implemented. Original damage
rounding, minimum damage, splash/falloff and immunity flags remain unverified or
unsupported; this is not complete original warhead behavior.

Full-game saves/replays and TCP handshakes are now v2. v1 full-game artifacts
and peers are rejected; there is no migration adapter. Movement/map checkpoints
and input-frame encoding remain v1. Imported armor and every multiplier are
included in the game state hash and engine saves.

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
| Primary/Secondary Damage | Nonnegative direct damage; healing weapons rejected |
| Primary/Secondary Range | Positive whole cells, at most 1024; fractional ranges rejected |
| Primary/Secondary ROF | `ceil(ROF * numerator / denominator)`, minimum one Tick, using `--rule-rof=N:D` |
| Power | Signed integer applied to the engine power balance |
| Harvester | Checked boolean; synthetic harvesting behavior with warning |
| BuildingTypes | Stationary definitions; no original construction/footprint behavior |
| AircraftTypes | Import fails; air movement is unavailable |
| Factory | Typed building factories; UnitType/InfantryType products enforced |
| Armor | Eleven standard armor slots; unknown/custom armor rejected |
| Warhead Verses | Exactly eleven percentages; exact thousandths of a percent; target armor scales direct damage |
| Prerequisite | Required groups: one owned building per group; exact IDs are singleton groups |
| Owner, Sight | Preserved with explicit omitted-behavior diagnostics |
| Projectile and other warhead properties | Immediate direct hits; other properties retained with diagnostics |
| Other properties | Retained by asset frontend, source/line diagnostics |

Build duration must be supplied explicitly with `--rule-build-ticks=N`; it is
not derived from original Cost/BuildSpeed semantics. Speed and ROF calibration
are caller choices, not verified RA2 timing conversions. Import does not clamp
invalid values, invent missing weapon definitions, or silently round ranges.

The game library exposes `rule_import::import` independently of the runtime.
The returned diagnostics must be shown to users before using experimental rules.
Runtime prints all diagnostics to stderr; the graphical scene keeps the first
100. Projectiles, full original weapon-selection semantics, prerequisites, faction
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

Full-game saves/replays and TCP handshakes are now v5. v1/v2/v3/v4 full-game artifacts
and peers are rejected; there is no migration adapter. Movement/map checkpoints
and input-frame encoding remain v1. Imported armor and every multiplier are
included in the game state hash and engine saves.


Secondary weapon experiment:

```sh
cargo run --release -p ra2ne-runtime -- --rules-experiment=fixtures/rules-experiment.ini --rules-overlay=fixtures/secondary-experiment.ini --rule-unit=TESTTANK --rule-speed=5:1 --rule-rof=1:1 --rule-build-ticks=90 --units=512 --autoplay --headless-ticks=900
```

The overlay makes the primary ineffective against heavy armor and supplies a
secondary weapon. Both slots share the same actor cooldown; the chosen weapon
sets its duration. Explicit attacks select the first nonzero-eligible weapon,
primary before secondary. Automatic acquisition selects the first weapon allowed
by passive-acquisition flags, using the same priority. Range is checked after
selection: a longer secondary range alone does not replace an eligible primary.
Pursuit stops at the selected weapon's range. Secondary-only types work.
Elite weapons, ammo, AA/AG filters, wall/deploy/transport special selection and
NoSecondaryWeaponFallback extensions remain unsupported. These are explicit
experimental rules, not complete original selection behavior.


Typed production now applies building prerequisite groups when reserving a job.
An enemy building does not satisfy ownership. A lost prerequisite pauses an
already-paid job without another charge, and restoring it resumes the job.
Power and owned-type membership are snapshotted once per Tick for production.
Queues retain their existing refund and blocked-exit behavior. Initial population
and scripted spawn do not require prerequisites.

`Factory=UnitType` and `Factory=InfantryType` identify supported factory product
categories. `AircraftType` and `BuildingType` are retained as typed factory
categories, but aircraft production/building placement are rejected. Factories
must be registered buildings. Unknown categories, naval units/factories, missing
prerequisite IDs and non-building prerequisites fail. Non-building alternates, faction Owner overlap, TechLevel, stolen tech, naval separation,
original production timing and construction remain unsupported. Synthetic legacy
factories without typed restrictions retain their fixture behavior.

The library fixture `fixtures/production-experiment.ini` exercises vehicle and
infantry factories with an exact laboratory prerequisite. It is covered by
`cargo test -p ra2ne-game imported_factories_enforce_categories_prerequisites_and_replay`.
The runtime can explicitly place factory/prerequisite buildings using the options
below; this test is not original skirmish acceptance.


General prerequisite aliases now resolve POWER/FACTORY/BARRACKS/RADAR/TECH/PROC
from the corresponding `[General] PrerequisitePower/Factory/Barracks/Radar/Tech/Proc`
list. A listed building is an alternative within that group; every group named
by a type must be satisfied. `[GenericPrerequisites]` supports custom group names
and overrides the base list of a standard group. Matching is ASCII case-insensitive.
Group members and groups canonicalize by stable definition index, removing
duplicates so reordered lists preserve gameplay definitions and hashes.

No original building defaults are invented: missing standard group definitions
fail. Empty groups, unknown or non-building members, nested groups and groups
larger than 1024 members fail. There may be at most 1024 groups per type and
1,000,000 total member references. PROC non-building alternate requirements
explicitly fail when used; generic alternate flags and Ares alternative-list,
negative/theater/stolen-tech/upgrade logic remain pending. No complete Ares
compatibility is claimed.

`fixtures/prerequisite-groups.ini` overlays `production-experiment.ini` with
standard and custom groups. Engine tests exercise group ownership and save/replay;
source tests exercise overrides and malformed definitions. The preview supports explicit experimental base placement and production controls;
an original build sidebar and construction remain pending.


## Experimental production scene

`--rule-building=ID` (repeatable, at most 32) places one registered building of
that type for each player in free walkable base slots. This places fixture
buildings directly; it does not implement construction or original footprints.
`--rule-queue=ID` (repeatable, at most 32) queues the product for each player at
the first eligible factory. Invalid IDs, wrong categories, missing prerequisites,
insufficient credits and unavailable base slots fail scene creation explicitly.
Each player starts with 2000 credits; initial orders reserve their full cost.

```sh
cargo run -p ra2ne-runtime -- --units=8 \
  --rules-experiment=fixtures/production-experiment.ini \
  --rule-unit=TESTTANK --rule-speed=5:1 --rule-rof=1:1 \
  --rule-build-ticks=90 \
  --rule-building=TESTFACTORY --rule-building=TESTBARRACKS \
  --rule-building=TESTLAB \
  --rule-queue=TESTTANK --rule-queue=TESTINFANTRY
```

The experimental sidebar uses the selected owned factory, or the first owned
factory when none is selected. Mobile definitions are paged with cost and
availability; hovering a disabled product shows the engine's rejection reason.
Click an available product to queue it, or cancel the first job for a full refund.
The queue displays its length and first job's remaining simulation ticks; low
power is displayed as a pause. B queues the first eligible product at that
factory. This is local preview input, not a multiplayer build interface.
Existing `--save-game=PATH` / `--load-game=PATH` preserve bases and paid queues.
Formats remain v5 because no serialized gameplay state changed.

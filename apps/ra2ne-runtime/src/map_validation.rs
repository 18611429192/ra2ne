//! Opt-in acceptance check against user-owned maps and stock resources.
//! CI never needs or distributes original game files.
use super::{installation, overlays, read};
use ra2ne_assets::{
    map::Ra2Map,
    normals,
    sprite::Palette,
    text::TextEncoding,
    theater::ResolvedTile,
    voxel::{Hva, Vxl},
    voxel_render,
    vpl::Vpl,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
const EXPECTED_NO_IMAGE: &str = include_str!("../tests/data/map_no_image_expected.tsv");
/// A tolerated no-image reference: kind, byte/word id, and frame/subtile.
type NoImageRef = (String, i32, u8);
fn collect(root: &Path, depth: usize, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 8 {
        return Err("map directory nesting exceeds eight levels".into());
    }
    for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            collect(&entry.path(), depth + 1, paths)?;
        } else if kind.is_file()
            && entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| ["map", "mpr", "yrm"].contains(&s.to_ascii_lowercase().as_str()))
        {
            paths.push(entry.path());
            if paths.len() > 4096 {
                return Err("map count exceeds 4096".into());
            }
        }
    }
    Ok(())
}
fn load(path: &Path, encoding: TextEncoding) -> Result<(String, Ra2Map), String> {
    let bytes = read(
        path.to_str().ok_or("map path is not UTF-8")?,
        16 * 1024 * 1024,
    )?;
    let text = encoding.decode(&bytes)?.into_owned();
    let map = Ra2Map::parse(&text)?;
    Ok((text, map))
}
fn expected_no_image() -> BTreeMap<String, BTreeSet<NoImageRef>> {
    let mut expected = BTreeMap::<String, BTreeSet<NoImageRef>>::new();
    for (number, line) in EXPECTED_NO_IMAGE.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        let [map, kind, id, index] = fields[..] else {
            panic!(
                "map_no_image_expected.tsv:{}: expected four tab-separated fields",
                number + 1
            );
        };
        assert!(
            [
                "terrain-missing",
                "terrain-noblock",
                "terrain-outside",
                "overlay-frame",
                "overlay-skip",
            ]
            .contains(&kind),
            "map_no_image_expected.tsv:{}: unknown kind {kind}",
            number + 1
        );
        expected.entry(map.to_owned()).or_default().insert((
            kind.to_owned(),
            id.parse().unwrap(),
            index.parse().unwrap(),
        ));
    }
    expected
}
#[derive(Default)]
struct Totals {
    terrain_images: usize,
    overlay_cells: usize,
    overlay_images: usize,
    empty_cells: usize,
    outside_cells: usize,
    infantry_actors: usize,
    infantry_unresolved: usize,
    infantry_failures: usize,
    scenery_cells: usize,
    scenery_images: usize,
    scenery_failures: usize,
    scenery_outside: usize,
    scenery_out_of_range: usize,
}
/// Resolve and decode every referenced image under the same rules the
/// original engine uses, recording the references that intentionally draw
/// nothing alongside any hard failures.
fn scan(
    game: &str,
    root: &str,
    edition: installation::Edition,
    encoding: TextEncoding,
) -> (BTreeMap<String, BTreeSet<NoImageRef>>, Vec<String>, Totals) {
    let mut paths = Vec::new();
    collect(Path::new(root), 0, &mut paths).unwrap();
    paths.sort();
    assert!(!paths.is_empty(), "no .map/.mpr/.yrm files found");
    let mut groups = BTreeMap::<String, Vec<PathBuf>>::new();
    for path in &paths {
        let (_, map) = load(path, encoding).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        groups.entry(map.theater).or_default().push(path.clone());
    }
    let mut actual = BTreeMap::<String, BTreeSet<NoImageRef>>::new();
    let mut failures = Vec::new();
    let mut totals = Totals::default();
    for (environment, paths) in groups {
        let (catalogue, _, files) =
            installation::load(game, edition, &environment, encoding).unwrap();
        let mut checked_palettes = BTreeSet::new();
        for path in &paths {
            let (text, map) = load(path, encoding).unwrap();
            let key = path
                .strip_prefix(root)
                .map_or_else(|_| path.clone(), Path::to_path_buf)
                .to_string_lossy()
                .replace('\\', "/");
            let refs = actual.entry(key).or_default();
            let mut tiles = BTreeMap::<i16, BTreeSet<u8>>::new();
            for tile in &map.tiles {
                tiles
                    .entry(tile.tile_index)
                    .or_default()
                    .insert(tile.sub_tile);
            }
            let scenery_error =
                match overlays::Scene::load_scenery(&files, &map, &text, edition, encoding) {
                    Ok(scene) => {
                        totals.scenery_cells += scene.statistics.cells;
                        totals.scenery_images += scene.statistics.images;
                        totals.scenery_outside += scene.statistics.outside;
                        totals.scenery_out_of_range += scene.statistics.out_of_range;
                        let structural = scene
                            .statistics
                            .no_image
                            .iter()
                            .filter(|(_, _, kind)| !matches!(kind, overlays::NoImage::EmptyFrame))
                            .map(|(id, frame, kind)| format!("{id}/{frame} {kind:?}"))
                            .collect::<Vec<_>>();
                        if scene.statistics.unresolved > 0 || !structural.is_empty() {
                            totals.scenery_failures += 1;
                            Some(format!(
                                "{} unresolved scenery records, structural no-image [{}]",
                                scene.statistics.unresolved,
                                structural.join(", ")
                            ))
                        } else {
                            None
                        }
                    }
                    Err(error) => {
                        totals.scenery_failures += 1;
                        Some(error)
                    }
                };
            let infantry_error =
                match super::infantry::Scene::load(&files, &map, &text, edition, encoding) {
                    Ok(scene) => {
                        totals.infantry_actors += scene.cells;
                        totals.infantry_unresolved += scene.unresolved;
                        if scene.unresolved > 0 {
                            totals.infantry_failures += 1;
                            Some(scene.report())
                        } else {
                            None
                        }
                    }
                    Err(error) => {
                        totals.infantry_failures += 1;
                        Some(error)
                    }
                };
            let check = (|| -> Result<(), String> {
                for (id, subtiles) in tiles {
                    match catalogue
                        .resolve(id, &files)
                        .map_err(|e| format!("terrain {id}: {e}"))?
                    {
                        ResolvedTile::Loaded { tmp, filename, .. } => {
                            if tmp.cell_width != 60 || tmp.cell_height != 30 {
                                return Err(format!("terrain {id}: non-60x30 cell"));
                            }
                            let palette = match filename.rsplit_once('.').unwrap().1 {
                                "tem" => "isotem.pal",
                                "sno" => "isosno.pal",
                                "urb" => "isourb.pal",
                                "ubn" => "isoubn.pal",
                                "des" => "isodes.pal",
                                "lun" => "isolun.pal",
                                _ => return Err("unknown resolved terrain extension".into()),
                            };
                            if checked_palettes.insert(palette) {
                                let file = files
                                    .get(palette)?
                                    .ok_or("resolved terrain palette missing")?;
                                Palette::parse(file.bytes)?;
                            }
                            for sub in subtiles {
                                match tmp
                                    .composite(usize::from(sub))
                                    .map_err(|e| format!("terrain {id}/{sub}: {e}"))?
                                {
                                    Some(_) => totals.terrain_images += 1,
                                    None => {
                                        refs.insert(("terrain-noblock".into(), i32::from(id), sub));
                                    }
                                }
                            }
                        }
                        ResolvedTile::Missing => {
                            for sub in subtiles {
                                refs.insert(("terrain-missing".into(), i32::from(id), sub));
                            }
                        }
                        ResolvedTile::Outside => {
                            for sub in subtiles {
                                refs.insert(("terrain-outside".into(), i32::from(id), sub));
                            }
                        }
                    }
                }
                let scene = overlays::Scene::load(&files, &map, &text, edition, encoding)?;
                totals.overlay_cells += scene.statistics.cells;
                totals.overlay_images += scene.statistics.images;
                totals.empty_cells += scene.statistics.empty;
                totals.outside_cells += scene.statistics.outside;
                for &(id, frame, kind) in &scene.statistics.no_image {
                    match kind {
                        overlays::NoImage::EmptyFrame => {}
                        overlays::NoImage::FrameOutOfRange => {
                            refs.insert(("overlay-frame".into(), i32::from(id), frame));
                        }
                        overlays::NoImage::EngineSkip => {
                            refs.insert(("overlay-skip".into(), i32::from(id), frame));
                        }
                    }
                }
                if scene.statistics.unresolved > 0 {
                    return Err(format!(
                        "{} unresolved overlay cells",
                        scene.statistics.unresolved
                    ));
                }
                Ok(())
            })();
            let mut errors = Vec::new();
            if let Err(error) = check {
                errors.push(error);
            }
            if let Some(error) = scenery_error {
                errors.push(error);
            }
            if let Some(error) = infantry_error {
                errors.push(error);
            }
            if !errors.is_empty() {
                failures.push(format!("{}: {}", path.display(), errors.join("; ")));
            }
        }
        println!(
            "map_graphics_theater={environment}; maps={}; failures_so_far={}",
            paths.len(),
            failures.len()
        );
    }
    (actual, failures, totals)
}
fn compare(
    expected: &BTreeMap<String, BTreeSet<NoImageRef>>,
    actual: &BTreeMap<String, BTreeSet<NoImageRef>>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (map, expected_refs) in expected {
        let Some(actual_refs) = actual.get(map) else {
            println!("map_no_image_expected.tsv: {map} was not scanned; skipped");
            continue;
        };
        for reference in expected_refs.difference(actual_refs) {
            problems.push(format!(
                "{map}: recorded no-image case vanished: {reference:?}"
            ));
        }
        for reference in actual_refs.difference(expected_refs) {
            problems.push(format!("{map}: unexpected no-image case: {reference:?}"));
        }
    }
    for (map, actual_refs) in actual {
        if expected.contains_key(map) {
            continue;
        }
        for reference in actual_refs {
            problems.push(format!("{map}: unexpected no-image case: {reference:?}"));
        }
    }
    problems
}
#[test]
#[ignore = "prints a classification of map parsing diagnostics for the checked map set"]
fn survey_map_parse_diagnostics() {
    let root = std::env::var("RA2NE_MAP_DIR").expect("set RA2NE_MAP_DIR");
    let encoding = TextEncoding::parse(
        &std::env::var("RA2NE_MAP_ENCODING").unwrap_or_else(|_| "windows1252".into()),
    )
    .unwrap();
    let mut paths = Vec::new();
    collect(Path::new(&root), 0, &mut paths).unwrap();
    paths.sort();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut details = Vec::new();
    let mut total = 0;
    for path in &paths {
        let (_, map) = load(path, encoding).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let key = path
            .strip_prefix(&root)
            .map_or_else(|_| path.clone(), Path::to_path_buf)
            .to_string_lossy()
            .replace('\\', "/");
        for diagnostic in &map.diagnostics {
            let label = format!("{}: {}", diagnostic.section, diagnostic.message);
            *counts.entry(label).or_default() += 1;
            total += 1;
            if !diagnostic.message.ends_with("runtime semantics pending") {
                details.push(format!(
                    "{key}:{}: {}: {}",
                    diagnostic.line, diagnostic.section, diagnostic.message
                ));
            }
        }
    }
    println!("map_parse_diagnostics={total}; maps={}", paths.len());
    for (key, count) in counts {
        println!("{count}\t{key}");
    }
    println!("--- detail:");
    for detail in details {
        println!("{detail}");
    }
}
#[test]
#[ignore = "requires RA2NE_GAME_DIR and RA2NE_MAP_DIR containing user-owned resources"]
fn all_private_map_graphics_resolve_and_decode() {
    let game = std::env::var("RA2NE_GAME_DIR").expect("set RA2NE_GAME_DIR");
    let root = std::env::var("RA2NE_MAP_DIR").expect("set RA2NE_MAP_DIR");
    let edition = installation::Edition::parse(
        &std::env::var("RA2NE_EDITION").unwrap_or_else(|_| "yr".into()),
    )
    .unwrap();
    let encoding = TextEncoding::parse(
        &std::env::var("RA2NE_MAP_ENCODING").unwrap_or_else(|_| "windows1252".into()),
    )
    .unwrap();
    let (actual, failures, totals) = scan(&game, &root, edition, encoding);
    println!(
        "map_graphics_maps={}; terrain_images={}; overlay_cells={}; overlay_images={}; empty_overlay_cells={}; outside_overlay_cells={}; tolerated_no_image_refs={}",
        actual.len(),
        totals.terrain_images,
        totals.overlay_cells,
        totals.overlay_images,
        totals.empty_cells,
        totals.outside_cells,
        actual.values().map(BTreeSet::len).sum::<usize>(),
    );
    println!(
        "map_scenery_cells={}; images={}; outside={}; out_of_range={}; failing_maps={}",
        totals.scenery_cells,
        totals.scenery_images,
        totals.scenery_outside,
        totals.scenery_out_of_range,
        totals.scenery_failures
    );
    println!(
        "map_infantry_actors={}; unresolved={}; failing_maps={}",
        totals.infantry_actors, totals.infantry_unresolved, totals.infantry_failures
    );
    let mut problems = compare(&expected_no_image(), &actual);
    problems.extend(failures);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
#[test]
#[ignore = "rewrites the frozen no-image table; review the diff before committing it"]
fn freeze_map_no_image_expectations() {
    let game = std::env::var("RA2NE_GAME_DIR").expect("set RA2NE_GAME_DIR");
    let root = std::env::var("RA2NE_MAP_DIR").expect("set RA2NE_MAP_DIR");
    let edition = installation::Edition::parse(
        &std::env::var("RA2NE_EDITION").unwrap_or_else(|_| "yr".into()),
    )
    .unwrap();
    let encoding = TextEncoding::parse(
        &std::env::var("RA2NE_MAP_ENCODING").unwrap_or_else(|_| "windows1252".into()),
    )
    .unwrap();
    let (actual, failures, _) = scan(&game, &root, edition, encoding);
    let mut table = String::from(
        "# Frozen tolerated no-image references for the checked installation.\n\
         # Every other referenced image must resolve and decode; regenerate with\n\
         # `cargo test -p ra2ne-runtime freeze_map_no_image_expectations -- --ignored`.\n\
         # map\tkind\tid\tindex\n",
    );
    for (map, refs) in &actual {
        for (kind, id, index) in refs {
            table.push_str(&format!("{map}\t{kind}\t{id}\t{index}\n"));
        }
    }
    std::fs::write("tests/data/map_no_image_expected.tsv", &table).unwrap();
    println!("wrote {} no-image references", table.lines().count() - 4);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
/// Stock voxels must stay inside their generation's normal table and shade
/// through the shipped `voxels.vpl` section tables.
#[test]
#[ignore = "requires RA2NE_GAME_DIR containing user-owned resources"]
fn stock_voxels_shade_through_engine_tables_and_vpl() {
    let game = std::env::var("RA2NE_GAME_DIR").expect("set RA2NE_GAME_DIR");
    let edition = installation::Edition::parse(
        &std::env::var("RA2NE_EDITION").unwrap_or_else(|_| "yr".into()),
    )
    .unwrap();
    let encoding = TextEncoding::parse(
        &std::env::var("RA2NE_MAP_ENCODING").unwrap_or_else(|_| "windows1252".into()),
    )
    .unwrap();
    let (_, _, files) = installation::load(&game, edition, "TEMPERATE", encoding).unwrap();
    let raw = files.get("voxels.vpl").unwrap().expect("stock voxels.vpl");
    assert_eq!(raw.bytes.len(), 16 + 768 + 32 * 256);
    let vpl = Vpl::parse(raw.bytes).expect("voxels.vpl layout");
    assert_eq!(vpl.section_count(), 32);
    assert_eq!(vpl.remap, [16, 31]);
    assert_eq!(vpl.palette[0], [0, 0, 0]);
    // Identity sections reproduce the raw voxel colours; the shipped tables
    // must differ from that, otherwise shading would be a no-op.
    let mut identity = Vec::new();
    identity.extend_from_slice(&16u32.to_le_bytes());
    identity.extend_from_slice(&31u32.to_le_bytes());
    identity.extend_from_slice(&32u32.to_le_bytes());
    identity.extend_from_slice(&0u32.to_le_bytes());
    identity.extend(std::iter::repeat_n(0u8, 768));
    for _ in 0..32u32 {
        identity.extend(0..=255u8);
    }
    let identity = Vpl::parse(&identity).unwrap();
    let mut models = 0;
    let mut voxels = 0;
    let mut shaded_models = 0;
    let mut generations = BTreeSet::new();
    for name in [
        "htnk", "mtnk", "ttnk", "fv", "sref", "hvr", "v3", "hind", "zep", "shad",
    ] {
        let Some(file) = files.get(&format!("{name}.vxl")).unwrap() else {
            continue;
        };
        let model = Vxl::parse(file.bytes).unwrap_or_else(|e| panic!("{name}.vxl: {e}"));
        models += 1;
        for limb in &model.limbs {
            let table = normals::table(limb.normal_mode)
                .unwrap_or_else(|| panic!("{name}: normal generation {}", limb.normal_mode));
            generations.insert(limb.normal_mode);
            for voxel in &limb.voxels {
                assert!(
                    usize::from(voxel.normal) < table.len(),
                    "{name}: normal {} outside the {}-entry generation {} table",
                    voxel.normal,
                    table.len(),
                    limb.normal_mode
                );
            }
            voxels += limb.voxels.len();
        }
        let Some(pose_file) = files.get(&format!("{name}.hva")).unwrap() else {
            continue;
        };
        let pose = Hva::parse(pose_file.bytes).unwrap_or_else(|e| panic!("{name}.hva: {e}"));
        let yaw = 0.5;
        let components = [voxel_render::Component {
            model: &model,
            pose: &pose,
            frame: 0,
            yaw,
        }];
        let unlit = voxel_render::render(&components, None).unwrap();
        let plain =
            voxel_render::render(&components, Some(&voxel_render::Shading { vpl: &identity }))
                .unwrap();
        assert_eq!(plain.image.pixels, unlit.image.pixels, "{name}: identity");
        let lit =
            voxel_render::render(&components, Some(&voxel_render::Shading { vpl: &vpl })).unwrap();
        assert_eq!(lit.image.pixels.len(), unlit.image.pixels.len());
        assert_ne!(
            lit.image.pixels, unlit.image.pixels,
            "{name}: shipped sections did not change the preview"
        );
        // Every drawn pixel must match the section one of this model's own
        // voxels resolves to under the engine's shading terms.
        let mut expected = BTreeSet::new();
        for limb in &model.limbs {
            let table = normals::table(limb.normal_mode).unwrap();
            let (light, up) = voxel_render::limb_light(yaw, &limb.transform);
            for voxel in &limb.voxels {
                if voxel.color == 0 {
                    continue;
                }
                let section = voxel_render::section(table[usize::from(voxel.normal)], light, up);
                expected.insert(vpl.shade(section, voxel.color));
            }
        }
        for pixel in &lit.image.pixels {
            assert!(
                *pixel == 0 || expected.contains(pixel),
                "{name}: pixel {pixel} outside the expected section colours"
            );
        }
        shaded_models += 1;
    }
    assert!(models >= 8, "only {models} stock voxels found");
    assert!(shaded_models >= 6, "only {shaded_models} voxels shaded");
    println!(
        "voxel_shading_models={models}; voxels={voxels}; shaded={shaded_models}; generations={generations:?}"
    );
}

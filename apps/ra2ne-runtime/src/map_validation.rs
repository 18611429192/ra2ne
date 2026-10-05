//! Opt-in acceptance check against user-owned maps and stock resources.
//! CI never needs or distributes original game files.
use super::{installation, overlays, read};
use ra2ne_assets::{map::Ra2Map, sprite::Palette, text::TextEncoding};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
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
    let mut paths = Vec::new();
    collect(Path::new(&root), 0, &mut paths).unwrap();
    paths.sort();
    assert!(!paths.is_empty(), "no .map/.mpr/.yrm files found");
    let mut groups = BTreeMap::<String, Vec<PathBuf>>::new();
    for path in &paths {
        let (_, map) = load(path, encoding).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        groups.entry(map.theater).or_default().push(path.clone());
    }
    let mut failures = Vec::new();
    let mut terrain_images = 0;
    let mut overlay_cells = 0;
    let mut empty_cells = 0;
    let mut outside_cells = 0;
    let mut overlay_images = 0;
    let mut scenery_cells = 0;
    let mut scenery_images = 0;
    let mut scenery_failures = 0;
    let mut scenery_outside = 0;
    for (environment, paths) in groups {
        let (catalogue, _, files) =
            installation::load(&game, edition, &environment, encoding).unwrap();
        let mut checked_palettes = BTreeSet::new();
        for path in &paths {
            let (text, map) = load(path, encoding).unwrap();
            let mut tiles = BTreeMap::<i16, BTreeSet<u8>>::new();
            for tile in &map.tiles {
                tiles
                    .entry(tile.tile_index)
                    .or_default()
                    .insert(tile.sub_tile);
            }
            let scenery = overlays::Scene::load_scenery(&files, &map, &text, edition, encoding);
            let scenery_error = match scenery {
                Ok(scene) => {
                    scenery_cells += scene.statistics.cells;
                    scenery_images += scene.statistics.images;
                    scenery_outside += scene.statistics.outside;
                    if scene.statistics.unresolved > 0 {
                        scenery_failures += 1;
                        Some(format!(
                            "{} unresolved scenery records",
                            scene.statistics.unresolved
                        ))
                    } else {
                        None
                    }
                }
                Err(error) => {
                    scenery_failures += 1;
                    Some(error)
                }
            };
            let check = (|| -> Result<(), String> {
                for (id, subtiles) in tiles {
                    let resolved = catalogue
                        .resolve(id, &files)
                        .map_err(|e| format!("terrain {id}: {e}"))?;
                    if resolved.tmp.cell_width != 60 || resolved.tmp.cell_height != 30 {
                        return Err(format!("terrain {id}: non-60x30 cell"));
                    }
                    let palette = match resolved.filename.rsplit_once('.').unwrap().1 {
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
                        resolved
                            .tmp
                            .composite(usize::from(sub))
                            .map_err(|e| format!("terrain {id}/{sub}: {e}"))?;
                        terrain_images += 1;
                    }
                }
                let scene = overlays::Scene::load(&files, &map, &text, edition, encoding)?;
                overlay_cells += scene.statistics.cells;
                overlay_images += scene.statistics.images;
                empty_cells += scene.statistics.empty;
                outside_cells += scene.statistics.outside;
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
    println!(
        "map_graphics_maps={}; terrain_images={terrain_images}; overlay_cells={overlay_cells}; overlay_images={overlay_images}; empty_overlay_cells={empty_cells}; outside_overlay_cells={outside_cells}; failures={}",
        paths.len(),
        failures.len()
    );
    println!(
        "map_scenery_cells={scenery_cells}; images={scenery_images}; outside={scenery_outside}; failing_maps={scenery_failures}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

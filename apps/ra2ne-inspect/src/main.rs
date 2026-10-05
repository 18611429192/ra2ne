use ra2ne_assets::{
    ini::Ini,
    map::Ra2Map,
    mix::{FilenameHash, MAX_ARCHIVE_BYTES, MixArchive},
    rules::RuleSet,
    text::TextEncoding,
};
use std::{env, fs::File, io::Read, process::ExitCode, sync::Arc};
mod audit;

fn read_bounded(path: &str, limit: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{path}: {e}"))?;
    if bytes.len() > limit {
        return Err(format!("{path}: file exceeds {limit} byte limit"));
    }
    Ok(bytes)
}
fn ini_report(bytes: &[u8], encoding: TextEncoding) -> Result<(), String> {
    let text = encoding.decode(bytes)?;
    let ini = Ini::parse(&text)?;
    println!(
        "ini_entries={}; diagnostics={}",
        ini.entries().len(),
        ini.diagnostics.len()
    );
    for d in &ini.diagnostics {
        println!("line={}; diagnostic={}", d.line, d.message);
    }
    Ok(())
}
fn map_report(bytes: &[u8], encoding: TextEncoding) -> Result<(), String> {
    let text = encoding.decode(bytes)?;
    let map = Ra2Map::parse(&text)?;
    println!(
        "map_name={}; theater={}; size={}x{}; terrain_cells={}; objects={}; waypoints={}; diagnostics={}",
        map.name,
        map.theater,
        map.size.width,
        map.size.height,
        map.tiles.len(),
        map.objects.len(),
        map.waypoints.len(),
        map.diagnostics.len()
    );
    for d in map.diagnostics.iter().take(100) {
        println!(
            "section={}; line={}; diagnostic={}",
            d.section, d.line, d.message
        );
    }
    if map.diagnostics.len() > 100 {
        println!("additional_diagnostics={}", map.diagnostics.len() - 100);
    }
    Ok(())
}
fn run() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("usage: ra2ne-inspect audit|theater|ini|map|rules|mix PATH [--file=NAME] [--nested=NAME,...] [--hash=classic|ra2] [--encoding=utf8|windows1252|gbk] [--overlay=PATH] [--extension=tem|sno|urb|des|ubn|lun]".into());
    }
    if args[0] == "audit" {
        if args.len() != 2 {
            return Err("usage: ra2ne-inspect audit GAME_DIRECTORY|RESOURCE_FILE".into());
        }
        return audit::run(&args[1]);
    }
    let mut extension = "tem";
    let mut file_name = None;
    let mut nested = None;
    let mut hash = FilenameHash::Ra2;
    let mut encoding = TextEncoding::Utf8;
    let mut overlays = Vec::new();
    for arg in &args[2..] {
        if let Some(value) = arg.strip_prefix("--extension=") {
            if args[0] != "theater" {
                return Err("extension requires theater mode".into());
            }
            extension = value;
        } else if let Some(value) = arg.strip_prefix("--overlay=") {
            overlays.push(value);
        } else if let Some(value) = arg.strip_prefix("--file=") {
            file_name = Some(value);
        } else if let Some(value) = arg.strip_prefix("--nested=") {
            nested = Some(value);
        } else if let Some(value) = arg.strip_prefix("--hash=") {
            hash = match value {
                "classic" => FilenameHash::Classic,
                "ra2" => FilenameHash::Ra2,
                _ => return Err("hash must be classic or ra2".into()),
            };
        } else if let Some(value) = arg.strip_prefix("--encoding=") {
            encoding = TextEncoding::parse(value)?;
        } else {
            return Err(format!("unknown option: {arg}"));
        }
    }
    if !overlays.is_empty() && args[0] != "rules" {
        return Err("overlay options require rules mode".into());
    }
    match args[0].as_str() {
        "theater" => {
            if file_name.is_some() || nested.is_some() {
                return Err("file/nested options require mix mode".into());
            }
            let bytes = read_bounded(&args[1], 16 * 1024 * 1024)?;
            let text = encoding.decode(&bytes)?;
            let theater = ra2ne_assets::theater::Theater::parse(&text, extension)?;
            println!("theater_tile_files={}", theater.tiles.len());
            for (id, tile) in theater.tiles.iter().enumerate() {
                println!(
                    "tile_id={id}; set={}; number={}; filename={}",
                    tile.set, tile.number, tile.filename
                );
            }
        }
        "map" => {
            if file_name.is_some() || nested.is_some() {
                return Err("file/nested options require mix mode".into());
            }
            map_report(&read_bounded(&args[1], 16 * 1024 * 1024)?, encoding)?;
        }
        "rules" => {
            if file_name.is_some() || nested.is_some() {
                return Err("file/nested options require mix mode".into());
            }
            let mut rules = RuleSet::default();
            for path in std::iter::once(args[1].as_str()).chain(overlays) {
                let bytes = read_bounded(path, 16 * 1024 * 1024)?;
                let text = encoding.decode(&bytes)?;
                rules.add_layer(path, &text)?;
            }
            let catalog = rules.discover()?;
            println!(
                "registered_types={}; rule_types={}; weapons={}; incomplete_types={}; incomplete_weapons={}; diagnostics={}",
                catalog.registered_type_count,
                catalog.types.len(),
                catalog.weapons.len(),
                catalog.incomplete_types.len(),
                catalog.incomplete_weapons.len(),
                catalog.diagnostics.len()
            );
            for d in catalog
                .incomplete_types
                .iter()
                .chain(&catalog.incomplete_weapons)
            {
                println!(
                    "source={}; line={}; section={}; incomplete={}",
                    d.source, d.line, d.section, d.message
                );
            }
            for d in catalog.diagnostics.iter().take(100) {
                println!(
                    "source={}; line={}; section={}; key={}; diagnostic={}",
                    d.source, d.line, d.section, d.key, d.message
                );
            }
            if catalog.diagnostics.len() > 100 {
                println!("additional_diagnostics={}", catalog.diagnostics.len() - 100);
            }
        }
        "ini" => {
            if file_name.is_some() || nested.is_some() {
                return Err("file/nested options require mix mode".into());
            }
            ini_report(&read_bounded(&args[1], 16 * 1024 * 1024)?, encoding)?;
        }
        "mix" => {
            let mut mix = MixArchive::parse(Arc::from(read_bounded(&args[1], MAX_ARCHIVE_BYTES)?))?;
            if let Some(names) = nested {
                let names: Vec<_> = names.split(',').collect();
                if names.len() > 8 {
                    return Err("nested archive depth exceeds 8".into());
                }
                for name in names {
                    let bytes = mix
                        .get(name, hash)?
                        .ok_or_else(|| format!("nested MIX not found: {name}"))?;
                    mix = MixArchive::parse(Arc::from(bytes))?;
                }
            }
            println!(
                "mix_entries={}; encrypted={}; checksum_verified={}",
                mix.entry_count(),
                mix.encrypted,
                mix.checksum_verified
            );
            if let Some(name) = file_name {
                let bytes = mix
                    .get(name, hash)?
                    .ok_or_else(|| format!("MIX file not found: {name}"))?;
                println!("resolved_name={name}; bytes={}", bytes.len());
                let extension = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                if ["map", "mpr", "yrm"].contains(&extension.as_str()) {
                    map_report(bytes, encoding)?;
                } else if extension == "ini" {
                    ini_report(bytes, encoding)?;
                }
            } else {
                for (id, length) in mix.entries() {
                    println!("id={id:08x}; bytes={length}");
                }
            }
        }
        _ => return Err("mode must be audit, ini, map, rules or mix".into()),
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ra2ne-inspect: {message}");
            ExitCode::FAILURE
        }
    }
}

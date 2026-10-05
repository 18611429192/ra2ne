//! Experimental owner palette ramps resolved from rules/map HSV colors.
//! The brightness-preserving ramp is a preview, not a verified stock algorithm.
use crate::{rules::RuleSet, sprite::Palette};

/// Remapable belongs to the art image section before an optional Image alias.
pub fn enabled(rules: &RuleSet, art: &RuleSet, type_id: &str) -> bool {
    let image = rules
        .get(type_id, "Image")
        .map_or(type_id, |v| v.entry.value.as_str());
    art.get(image, "Remapable").is_none_or(|v| {
        !matches!(
            v.entry.value.trim().to_ascii_lowercase().as_str(),
            "no" | "false" | "0"
        )
    })
}
pub fn house_color(rules: &RuleSet, house: &str) -> Result<Option<[u8; 3]>, &'static str> {
    let Some(color) = rules.get(house, "Color") else {
        return Ok(None);
    };
    let hsv = rules
        .get("Colors", color.entry.value.trim())
        .ok_or("unknown house color label")?;
    let values = hsv
        .entry
        .value
        .split(',')
        .map(|v| {
            v.trim()
                .parse::<u8>()
                .map_err(|_| "invalid house HSV component")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() != 3 {
        return Err("house HSV requires three components");
    }
    Ok(Some(hsv_rgb([values[0], values[1], values[2]])))
}
fn hsv_rgb(hsv: [u8; 3]) -> [u8; 3] {
    let h = u32::from(hsv[0]) * 6;
    let sector = h / 256;
    let f = h % 256;
    let s = u32::from(hsv[1]);
    let v = u32::from(hsv[2]);
    let p = v * (255 - s) / 255;
    let q = v * (255 - s * f / 256) / 255;
    let t = v * (255 - s * (256 - f) / 256) / 255;
    match sector {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
    .map(|v| v as u8)
}
/// Only the caller-specified inclusive palette range changes; transparency stays
/// attached to index zero. The source ramp's relative brightness is retained.
pub fn palette(source: &Palette, range: [u8; 2], rgb: [u8; 3]) -> Result<Palette, &'static str> {
    if range[0] == 0 || range[0] > range[1] {
        return Err("invalid remap palette range");
    }
    let mut result = source.clone();
    let peak = source.colors[usize::from(range[0])..=usize::from(range[1])]
        .iter()
        .flat_map(|c| c.iter())
        .copied()
        .max()
        .unwrap_or(0);
    if peak == 0 {
        return Ok(result);
    }
    for index in range[0]..=range[1] {
        let brightness = *source.colors[usize::from(index)].iter().max().unwrap();
        result.colors[usize::from(index)] =
            rgb.map(|v| (u32::from(v) * u32::from(brightness) / u32::from(peak)) as u8);
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remap_flag_is_checked_before_image_alias() {
        let mut rules = RuleSet::default();
        let mut art = RuleSet::default();
        rules.add_layer("rules", "[E1]\nImage=GI").unwrap();
        art.add_layer("art", "[GI]\nImage=ALIAS\nRemapable=no")
            .unwrap();
        assert!(!enabled(&rules, &art, "E1"));
        assert!(enabled(&rules, &art, "UNKNOWN"));
        art.add_layer("override", "[GI]\nRemapable=yes").unwrap();
        assert!(enabled(&rules, &art, "E1"));
    }
    #[test]
    fn aliases_overrides_invalid_and_unknown_houses() {
        let mut r = RuleSet::default();
        r.add_layer(
            "stock",
            "[Americans]\nColor=Gold\n[Colors]\nGold=43,239,255\nBlue=170,255,255",
        )
        .unwrap();
        assert!(house_color(&r, "Americans").unwrap().unwrap()[0] > 200);
        assert_eq!(house_color(&r, "Neutral").unwrap(), None);
        r.add_layer("map", "[Americans]\nColor=Blue").unwrap();
        let c = house_color(&r, "Americans").unwrap().unwrap();
        assert!(c[2] > 250 && c[0] < 10);
        r.add_layer("invalid", "[Colors]\nBlue=256,1,1").unwrap();
        assert!(house_color(&r, "Americans").is_err());
    }
    #[test]
    fn ramp_changes_only_range_preserving_alpha_and_brightness() {
        let mut p = Palette {
            colors: [[33; 3]; 256],
        };
        p.colors[16] = [240, 0, 0];
        p.colors[17] = [120, 0, 0];
        let q = palette(&p, [16, 17], [0, 0, 200]).unwrap();
        assert_eq!(q.colors[16], [0, 0, 200]);
        assert_eq!(q.colors[17], [0, 0, 100]);
        assert_eq!(q.colors[15], p.colors[15]);
        assert_eq!(q.colors[18], p.colors[18]);
        assert_eq!(q.colors[0], p.colors[0]);
        assert!(palette(&p, [0, 17], [1; 3]).is_err());
        assert!(palette(&p, [31, 16], [1; 3]).is_err());
    }
}

//! Stock owner-color ramps.
//!
//! `gamemd` builds a 16-shade ramp per `[Colors]` entry into unit-palette
//! indices 16..=31 (`FUN_0068c3b0`); the shade list is rebuilt whenever the
//! theater reloads (`FUN_005349c0` -> `FUN_0068c860`) and drawn through a
//! `ConvertClass` made from the rebuilt palette. The shades walk two fixed
//! angle curves through the engine's 8192-entry sine table and convert the
//! resulting HSV back with the integer conversion in `FUN_00517440`; see
//! docs/FORMAT_NOTES.md and the P1 analysis log in docs/REA_SETUP.md.
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
/// House `Color=` label resolved through `[Colors]` to the raw HSV triple the
/// engine copies into the color-scheme object (bytes `H,S,V`, each 0..=255).
pub fn house_hsv(rules: &RuleSet, house: &str) -> Result<Option<[u8; 3]>, &'static str> {
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
    Ok(Some([values[0], values[1], values[2]]))
}
/// `sin(angle2(i))` for the 16 ramp shades, read from the shipped sine table
/// at 0x84f084 (`f32` `sin(2*pi*k/8192)`). The ramp angles are fixed, so the
/// table lookups collapse to these constants; `angle2(i) = i *
/// 0.046542113386515455 + 0.8726646259971648` (50° to 90°) scaled by
/// `2607.594482421875` (= 16384/2PI) with `ftol(angle * scale) / 2` rounded
/// to the nearest entry (`FUN_004cacb0`).
const RAMP_SIN: [f32; 16] = [
    f32::from_bits(0x3f44_22aa),
    f32::from_bits(0x3f4b_7f08),
    f32::from_bits(0x3f52_89b4),
    f32::from_bits(0x3f59_1e6a),
    f32::from_bits(0x3f5f_397a),
    f32::from_bits(0x3f64_c0ee),
    f32::from_bits(0x3f69_e0d7),
    f32::from_bits(0x3f6e_7db7),
    f32::from_bits(0x3f72_84e4),
    f32::from_bits(0x3f76_167a),
    f32::from_bits(0x3f79_1e30),
    f32::from_bits(0x3f7b_9107),
    f32::from_bits(0x3f7d_8284),
    f32::from_bits(0x3f7e_e5f9),
    f32::from_bits(0x3f7f_b848),
    f32::from_bits(0x3f80_0000),
];
/// `cos(angle1(i))`, read 0x800 sine entries further (`FUN_004cad00`). The
/// first shade is special-cased to `PI/16`; the rest walk `angle1(i) = i *
/// 0.08144869842640204 + 0.3490658503988659` (20° to 90°).
const RAMP_COS: [f32; 16] = [
    f32::from_bits(0x3f7b_14be),
    f32::from_bits(0x3f68_aa48),
    f32::from_bits(0x3f5f_397a),
    f32::from_bits(0x3f54_330f),
    f32::from_bits(0x3f47_de65),
    f32::from_bits(0x3f3a_37b6),
    f32::from_bits(0x3f2b_561a),
    f32::from_bits(0x3f1b_52bb),
    f32::from_bits(0x3f0a_1e5d),
    f32::from_bits(0x3ef0_50c4),
    f32::from_bits(0x3eca_ce62),
    f32::from_bits(0x3ea3_f505),
    f32::from_bits(0x3e78_0cbd),
    f32::from_bits(0x3e25_c58b),
    f32::from_bits(0x3da6_522f),
    f32::from_bits(0x250d_3000),
];
/// The 16 owner shades written to palette indices 16..=31, brightest first:
/// `sat = ftol(sin(angle2(i)) * S)` and `val = ftol(cos(angle1(i)) * V)`,
/// with the hue kept constant. The last shade of every stock color is black.
pub fn ramp(hsv: [u8; 3]) -> [[u8; 3]; 16] {
    std::array::from_fn(|i| {
        let sat = (f64::from(RAMP_SIN[i]) * f64::from(hsv[1])) as u8;
        let val = (f64::from(RAMP_COS[i]) * f64::from(hsv[2])) as u8;
        hsv_rgb([hsv[0], sat, val])
    })
}
/// Engine HSV-to-RGB (`FUN_00517440`): sector `(6h)/255` with the remainder
/// as the fraction, six sector permutations of `p`/`q`/`t`, and all products
/// divided by 255 with truncation.
fn hsv_rgb(hsv: [u8; 3]) -> [u8; 3] {
    let h = u32::from(hsv[0]) * 6;
    let sector = h / 255;
    let f = h % 255;
    let s = u32::from(hsv[1]);
    let v = u32::from(hsv[2]);
    let p = (255 - s) * v / 255;
    let q = (255 - s * f / 255) * v / 255;
    let t = (255 - s * (255 - f) / 255) * v / 255;
    match sector {
        0 | 6 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
    .map(|value| value as u8)
}
/// Palette clone with the stock owner ramp in indices 16..=31. The shipped
/// unit palettes carry a legacy red ramp there; the engine always overwrites
/// exactly this range and leaves every other index (including index zero)
/// untouched.
pub fn palette(source: &Palette, hsv: [u8; 3]) -> Palette {
    let mut result = source.clone();
    for (index, rgb) in ramp(hsv).iter().enumerate() {
        result.colors[16 + index] = *rgb;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    /// The stock `[Colors]` ramps, generated from the shipped executable's own
    /// sine-table bytes (see docs/REA_SETUP.md); each hex string is the
    /// 16-shade `palette[16..=31]` range in RGB triples.
    const STOCK_RAMPS: &[(&str, [u8; 3], &str)] = &[
        (
            "LightGold",
            [25, 255, 255],
            "faab3ae79b30de9328d38a20c77f19b97514aa6a0f9a5f0a895307774804653c025130013d2400291800140b00000000",
        ),
        (
            "Gold",
            [41, 240, 230],
            "e1db3fd1cc35c8c32dbeb826b3ae20a7a21a9994158b87117c780d6b670a5b5807494605373503252302121101000000",
        ),
        (
            "LightGrey",
            [0, 0, 240],
            "ebebebdadadad1d1d1c6c6c6bbbbbbaeaeaea0a0a09191918181817070705f5f5f4c4c4c3a3a3a262626131313000000",
        ),
        (
            "Grey",
            [0, 0, 131],
            "8080807777777272726c6c6c6666665f5f5f5757574f4f4f4646463d3d3d3333332929291f1f1f1515150a0a0a000000",
        ),
        (
            "Red",
            [20, 255, 184],
            "b46b2aa76122a05a1c9854178f4d1285460e7b400b6f3807633105562a034823013a1b012c15001d0d000e0600000000",
        ),
        (
            "DarkRed",
            [0, 230, 255],
            "fa4d4de74242de3939d33131c72a2ab92424aa1e1e9a1818891414770f0f650c0c5109093d0606290404140202000000",
        ),
        (
            "Orange",
            [25, 230, 255],
            "fab34de7a342de9a39d39031c7872ab97c24aa701e9a6418895914774c0f65400c5133093d2606291904140c02000000",
        ),
        (
            "Magenta",
            [221, 102, 255],
            "faadebe79dd8de95d0d38bc4c782b9b976abaa6c9e9a608e89557e77496e653d5d51314a3d2438291825140c12000000",
        ),
        (
            "Purple",
            [201, 201, 189],
            "9b4ab98e40ab8739a47f349c772e936f288965237e5b1e725019654615583a114a2f0d3c23092d17061e0b030f000000",
        ),
        (
            "LightBlue",
            [119, 143, 255],
            "8ffae580e7d378dec96ed3bf66c7b45cb9a653aa98499a8a40897a37776a2d655a2451481b3d36122924081411000000",
        ),
        (
            "DarkBlue",
            [153, 214, 212],
            "4a80cf4073c0396cb83264af2c5da526559a214c8d1c4480173b721333630f2a530b2143081933051122020811000000",
        ),
        (
            "NeonBlue",
            [185, 156, 238],
            "a27ce9946ed88c67cf835fc57956b9704ead66469f5b3e90503680452e6f3a265e2e1e4c221639170e260b0713000000",
        ),
        (
            "DarkSky",
            [131, 200, 230],
            "5ad6e14fc6d147bdc840b4be38a9b3329da72b909925828b20747c1a646b15555b1044490c3337082225031012000000",
        ),
        (
            "Green",
            [104, 241, 195],
            "35bf732cb16826aa6120a15a1a9852168d4b1182440e763d0b6935085b2d064d26043e1e032f16011f0e000f07000000",
        ),
        (
            "DarkGreen",
            [81, 200, 210],
            "5ecd5253be484cb74145ae3a3ea33338982d318c282b7f2225711d1f62181a531314430f0e320b092107051103000000",
        ),
        (
            "NeonGreen",
            [0, 0, 0],
            "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        ),
        (
            "Yellow",
            [38, 159, 255],
            "faee83e7db74ded26cd3c764c7bc5bb9ae51aaa0499a9040898038776f30655e28514b1f3d391729260f141207000000",
        ),
    ];
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
    fn house_labels_resolve_aliases_overrides_and_invalid_colors() {
        let mut r = RuleSet::default();
        r.add_layer(
            "stock",
            "[Americans]\nColor=Gold\n[Colors]\nGold=43,239,255\nBlue=170,255,255",
        )
        .unwrap();
        assert_eq!(house_hsv(&r, "Americans").unwrap().unwrap(), [43, 239, 255]);
        assert_eq!(house_hsv(&r, "Neutral").unwrap(), None);
        r.add_layer("map", "[Americans]\nColor=Blue").unwrap();
        assert_eq!(
            house_hsv(&r, "Americans").unwrap().unwrap(),
            [170, 255, 255]
        );
        r.add_layer("invalid", "[Colors]\nBlue=256,1,1").unwrap();
        assert!(house_hsv(&r, "Americans").is_err());
        r.add_layer("partial", "[Colors]\nBlue=1,2").unwrap();
        assert!(house_hsv(&r, "Americans").is_err());
    }
    #[test]
    fn ramp_reproduces_the_stock_generator_for_every_colors_entry() {
        for (name, hsv, hex) in STOCK_RAMPS {
            let expected = decode_hex(hex);
            assert_eq!(expected.len(), 16, "{name}");
            let generated: Vec<[u8; 3]> = ramp(*hsv).into_iter().collect();
            assert_eq!(generated, expected, "{name}");
            assert_eq!(generated[15], [0, 0, 0], "{name} last shade");
        }
    }
    fn decode_hex(hex: &str) -> Vec<[u8; 3]> {
        hex.as_bytes()
            .chunks(6)
            .map(|chunk| [0, 2, 4].map(|k| hex_digit(chunk[k]) << 4 | hex_digit(chunk[k + 1])))
            .collect()
    }
    fn hex_digit(byte: u8) -> u8 {
        (byte as char).to_digit(16).unwrap() as u8
    }
    #[test]
    fn ramp_trig_constants_follow_the_shipped_table_model() {
        // Every shade's sat/val scale comes from these two indexes into the
        // engine's 8192-entry float table; recompute them from the angle
        // formulas to keep the constants honest.
        for (i, value) in RAMP_SIN.iter().enumerate() {
            let angle2 = i as f64 * 0.046542113386515455 + 0.8726646259971648;
            let v = (angle2 * 2607.594482421875) as i64;
            let mut index = v / 2;
            if v & 1 != 0 && index < 0x1FFF {
                index += 1;
            }
            let expected = (std::f64::consts::TAU * index as f64 / 8192.0).sin() as f32;
            assert!((value - expected).abs() < 1e-6, "sin shade {i}");
        }
        for (i, value) in RAMP_COS.iter().enumerate() {
            let angle1 = if i == 0 {
                std::f64::consts::PI / 16.0
            } else {
                i as f64 * 0.08144869842640204 + 0.3490658503988659
            };
            let v = (angle1 * 2607.594482421875) as i64;
            let mut index = v / 2 + 0x800;
            if v & 1 != 0 && index < 0x27FF {
                index += 1;
            }
            let expected = (std::f64::consts::TAU * index as f64 / 8192.0).sin() as f32;
            assert!((value - expected).abs() < 1e-6, "cos shade {i}");
        }
        // Both curves end at 90 degrees: sin exactly one, cos one rounding
        // step above zero (the table stores 1.2246e-16).
        let (end_sin, end_cos) = (RAMP_SIN[15], RAMP_COS[15]);
        assert_eq!(end_sin, 1.0);
        assert!(end_cos < 1e-6, "cos ends within one rounding step of zero");
    }
    #[test]
    fn hsv_rgb_follows_the_engine_sector_table() {
        assert_eq!(hsv_rgb([0, 255, 255]), [255, 0, 0]);
        assert_eq!(hsv_rgb([63, 255, 255]), [132, 255, 0]);
        assert_eq!(hsv_rgb([85, 255, 255]), [0, 255, 0]);
        assert_eq!(hsv_rgb([128, 255, 255]), [0, 252, 255]);
        assert_eq!(hsv_rgb([170, 255, 255]), [0, 0, 255]);
        assert_eq!(hsv_rgb([213, 255, 255]), [255, 0, 252]);
        assert_eq!(hsv_rgb([255, 255, 255]), [255, 0, 0]);
        assert_eq!(hsv_rgb([0, 0, 255]), [255, 255, 255]);
    }
    #[test]
    fn palette_overwrites_only_the_stock_remap_range() {
        let mut source = Palette {
            colors: [[33; 3]; 256],
        };
        source.colors[0] = [7, 8, 9];
        source.colors[16] = [63, 0, 0];
        source.colors[31] = [8, 0, 0];
        let result = palette(&source, [0, 0, 240]);
        assert_eq!(result.colors[0], [7, 8, 9]);
        assert_eq!(result.colors[15], [33; 3]);
        assert_eq!(result.colors[16], [235, 235, 235]);
        assert_eq!(result.colors[30], [19, 19, 19]);
        assert_eq!(result.colors[31], [0, 0, 0]);
        assert_eq!(result.colors[32], [33; 3]);
        assert_eq!(result.colors[255], [33; 3]);
    }
}

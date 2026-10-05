//! Static infantry standing poses. Animation and owner remap are separate work.
use crate::rules::RuleSet;
#[derive(Debug, Clone)]
pub struct StandingPose {
    pub image: String,
    pub start: usize,
    pub count: usize,
    pub stride: usize,
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
impl StandingPose {
    pub fn from_rules(type_id: &str, rules: &RuleSet, art: &RuleSet) -> Result<Self, &'static str> {
        if !name(type_id) {
            return Err("invalid infantry type ID");
        }
        if rules.section(type_id).is_empty()
            && !rules
                .section("InfantryTypes")
                .iter()
                .any(|v| v.entry.value.eq_ignore_ascii_case(type_id))
        {
            return Err("unregistered infantry type");
        }
        let art_id = rules
            .get(type_id, "Image")
            .map_or(type_id, |v| v.entry.value.as_str());
        let image = art
            .get(art_id, "Image")
            .map_or(art_id, |v| v.entry.value.as_str());
        if !name(art_id) || !name(image) {
            return Err("invalid infantry image prefix");
        }
        for flag in ["Voxel", "NewTheater", "TerrainPalette"] {
            if art
                .get(art_id, flag)
                .is_some_and(|v| v.entry.boolean().unwrap_or(true))
            {
                return Err("unsupported infantry art flags");
            }
        }
        if art.get(art_id, "Palette").is_some() {
            return Err("custom infantry palette is not implemented");
        }
        let sequence = art
            .get(art_id, "Sequence")
            .ok_or("missing infantry Sequence")?;
        if !name(&sequence.entry.value) {
            return Err("invalid infantry Sequence name");
        }
        let ready = art
            .get(&sequence.entry.value, "Ready")
            .ok_or("missing infantry Ready sequence")?;
        let parts = ready
            .entry
            .value
            .split(',')
            .map(str::trim)
            .collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err("unsupported Ready sequence fields");
        }
        let parts = parts
            .iter()
            .map(|v| {
                v.parse::<usize>()
                    .map_err(|_| "invalid Ready sequence number")
            })
            .collect::<Result<Vec<_>, _>>()?;
        if parts[1] == 0
            || parts.iter().any(|&n| n >= 10000)
            || parts[0]
                .checked_add(7 * parts[2])
                .and_then(|n| n.checked_add(parts[1]))
                .is_none_or(|n| n > 10000)
        {
            return Err("Ready sequence exceeds SHP frame limits");
        }
        Ok(Self {
            image: image.into(),
            start: parts[0],
            count: parts[1],
            stride: parts[2],
        })
    }
    /// The published RA2 editor uses (7 - direction/32) for infantry facings.
    /// Select the first frame of Ready, preserving stride-zero static poses.
    pub fn frame(&self, facing: u8) -> usize {
        self.start + (7 - usize::from(facing) / 32) * self.stride
    }
}
/// Projected infantry subcells, relative to the complete cell's center.
pub fn subcell_offset(subcell: u8) -> Result<(i32, i32), &'static str> {
    match subcell {
        0 => Ok((0, -7)),
        1 => Ok((15, 0)),
        2 => Ok((-15, 0)),
        3 => Ok((0, 7)),
        4 => Ok((0, 0)),
        _ => Err("invalid infantry subcell"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rules(text: &str) -> RuleSet {
        let mut r = RuleSet::default();
        r.add_layer("synthetic", text).unwrap();
        r
    }
    #[test]
    fn aliases_ready_stride_and_all_facing_boundaries() {
        let r = rules("[InfantryTypes]\n1=E1\n[E1]\nImage=GI");
        let a = rules("[GI]\nImage=ALIAS\nSequence=Stand\n[Stand]\nReady=10,2,3");
        let p = StandingPose::from_rules("e1", &r, &a).unwrap();
        assert_eq!(p.image, "ALIAS");
        let mut overridden = rules("[InfantryTypes]\n1=E1\n[E1]\nImage=GI");
        overridden
            .add_layer("map", "[InfantryTypes]\n1=OTHER")
            .unwrap();
        assert_eq!(
            StandingPose::from_rules("E1", &overridden, &a)
                .unwrap()
                .image,
            "ALIAS"
        );
        for f in 0..=255_u8 {
            assert_eq!(p.frame(f), 10 + (7 - usize::from(f) / 32) * 3);
        }
        for bad in ["-1,1,1", "0,0,1", "9999,1,1", "0,1,1,S", "0,1,broken"] {
            let a = rules(&format!("[GI]\nSequence=S\n[S]\nReady={bad}"));
            assert!(StandingPose::from_rules("E1", &r, &a).is_err());
        }
        assert!(StandingPose::from_rules("UNKNOWN", &r, &a).is_err());
        assert_eq!(subcell_offset(0), Ok((0, -7)));
        assert_eq!(subcell_offset(4), Ok((0, 0)));
        assert!(subcell_offset(5).is_err());
    }
}

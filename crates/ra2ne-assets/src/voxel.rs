//! Bounded VXL geometry and HVA poses. No rasterizer or gameplay interpretation.
use std::collections::BTreeSet;
const MAX_FILE: usize = 64 * 1024 * 1024;
const MAX_VOXELS: usize = 4_194_304;
const MAX_SECTIONS: usize = 256;
fn u32_at(bytes: &[u8], at: usize) -> Result<u32, &'static str> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("truncated voxel data")?
            .try_into()
            .unwrap(),
    ))
}
fn float_at(bytes: &[u8], at: usize) -> Result<f32, &'static str> {
    let value = f32::from_bits(u32_at(bytes, at)?);
    if value.is_finite() {
        Ok(value)
    } else {
        Err("non-finite voxel transform or bounds")
    }
}
fn name(bytes: &[u8]) -> Result<String, &'static str> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    if bytes[..end].iter().any(|&b| !(32..=126).contains(&b)) {
        return Err("invalid voxel section name");
    }
    Ok(String::from_utf8(bytes[..end].to_vec()).unwrap())
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub rows: [[f32; 4]; 3],
}
impl Transform {
    fn read(bytes: &[u8], at: usize) -> Result<Self, &'static str> {
        let mut rows = [[0.0; 4]; 3];
        for (r, row) in rows.iter_mut().enumerate() {
            for (c, value) in row.iter_mut().enumerate() {
                *value = float_at(bytes, at + (r * 4 + c) * 4)?;
            }
        }
        Ok(Self { rows })
    }
    pub fn apply(&self, point: [f32; 3]) -> Result<[f32; 3], &'static str> {
        let result = self
            .rows
            .map(|r| r[0] * point[0] + r[1] * point[1] + r[2] * point[2] + r[3]);
        if result.iter().all(|v| v.is_finite()) {
            Ok(result)
        } else {
            Err("voxel transform result is not finite")
        }
    }
}
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Voxel {
    pub position: [u8; 3],
    pub color: u8,
    pub normal: u8,
}
#[derive(Debug, Clone)]
pub struct Limb {
    pub name: String,
    pub index: u32,
    pub header_flags: [u32; 2],
    pub dimensions: [u8; 3],
    pub scale: f32,
    pub transform: Transform,
    pub bounds: [[f32; 3]; 2],
    pub normal_mode: u8,
    pub voxels: Vec<Voxel>,
}
#[derive(Debug, Clone)]
pub struct Vxl {
    pub remap: [u8; 2],
    pub palette: [[u8; 3]; 256],
    pub limbs: Vec<Limb>,
}
impl Vxl {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 802 || bytes.len() > MAX_FILE || &bytes[..16] != b"Voxel Animation\0" {
            return Err("invalid VXL header or size");
        }
        let count = u32_at(bytes, 20)? as usize;
        if u32_at(bytes, 16)? != 1
            || count == 0
            || count > MAX_SECTIONS
            || u32_at(bytes, 24)? as usize != count
        {
            return Err("invalid VXL section counts");
        }
        let body_size = u32_at(bytes, 28)? as usize;
        let body_start = 802 + count * 28;
        let footer_start = body_start
            .checked_add(body_size)
            .ok_or("VXL size overflow")?;
        if footer_start.checked_add(count * 92) != Some(bytes.len()) {
            return Err("VXL body/footer size mismatch");
        }
        let body = &bytes[body_start..footer_start];
        let mut limbs = Vec::with_capacity(count);
        let mut total = 0;
        for index in 0..count {
            let h = 802 + index * 28;
            let f = footer_start + index * 92;
            let dimensions: [u8; 3] = bytes[f + 88..f + 91].try_into().unwrap();
            if dimensions.contains(&0) {
                return Err("zero VXL dimensions");
            }
            let columns = usize::from(dimensions[0]) * usize::from(dimensions[1]);
            let start = u32_at(bytes, f)? as usize;
            let end = u32_at(bytes, f + 4)? as usize;
            let data = u32_at(bytes, f + 8)? as usize;
            if start
                .checked_add(columns * 4)
                .is_none_or(|n| n > body.len())
                || end.checked_add(columns * 4).is_none_or(|n| n > body.len())
                || data > body.len()
                || start + columns * 4 > end
                || end + columns * 4 > data
            {
                return Err("VXL span tables outside body or overlapping");
            }
            let scale = float_at(bytes, f + 12)?;
            if scale <= 0.0 {
                return Err("non-positive VXL scale");
            }
            let mut bounds = [[0.0; 3]; 2];
            for (i, vector) in bounds.iter_mut().enumerate() {
                for (j, v) in vector.iter_mut().enumerate() {
                    *v = float_at(bytes, f + 64 + (i * 3 + j) * 4)?;
                }
            }
            if (0..3).any(|i| bounds[0][i] > bounds[1][i]) {
                return Err("inverted VXL bounds");
            }
            let mut voxels = Vec::new();
            for col in 0..columns {
                let a = u32_at(body, start + col * 4)? as i32;
                let b = u32_at(body, end + col * 4)? as i32;
                if a == -1 && b == -1 {
                    continue;
                }
                if a < 0 || b < a {
                    return Err("invalid VXL column offsets");
                }
                let from = data
                    .checked_add(a as usize)
                    .ok_or("VXL column offset overflow")?;
                let to = data
                    .checked_add(b as usize)
                    .and_then(|v| v.checked_add(1))
                    .ok_or("VXL column offset overflow")?;
                let span = body.get(from..to).ok_or("VXL column outside body")?;
                let mut cursor = 0;
                let mut z = 0_usize;
                while z < usize::from(dimensions[2]) {
                    let pair = span.get(cursor..cursor + 2).ok_or("truncated VXL run")?;
                    cursor += 2;
                    let skip = usize::from(pair[0]);
                    let length = usize::from(pair[1]);
                    if skip + length == 0 || z + skip + length > usize::from(dimensions[2]) {
                        return Err("invalid VXL run height or progress");
                    }
                    z += skip;
                    let payload = span
                        .get(cursor..cursor + 2 * length)
                        .ok_or("truncated VXL voxel run")?;
                    cursor += 2 * length;
                    if span.get(cursor).copied() != Some(pair[1]) {
                        return Err("VXL run count trailer mismatch");
                    }
                    cursor += 1;
                    total += length;
                    if total > MAX_VOXELS {
                        return Err("VXL decoded voxel budget exceeded");
                    }
                    for element in payload.as_chunks::<2>().0 {
                        voxels.push(Voxel {
                            position: [
                                (col % usize::from(dimensions[0])) as u8,
                                (col / usize::from(dimensions[0])) as u8,
                                z as u8,
                            ],
                            color: element[0],
                            normal: element[1],
                        });
                        z += 1;
                    }
                }
                if cursor != span.len() {
                    return Err("unexpected bytes after VXL column");
                }
            }
            limbs.push(Limb {
                name: name(&bytes[h..h + 16])?,
                index: u32_at(bytes, h + 16)?,
                header_flags: [u32_at(bytes, h + 20)?, u32_at(bytes, h + 24)?],
                dimensions,
                scale,
                transform: Transform::read(bytes, f + 16)?,
                bounds,
                normal_mode: bytes[f + 91],
                voxels,
            });
        }
        Ok(Self {
            remap: [bytes[32], bytes[33]],
            palette: std::array::from_fn(|i| bytes[34 + i * 3..37 + i * 3].try_into().unwrap()),
            limbs,
        })
    }
    pub fn voxel_count(&self) -> usize {
        self.limbs.iter().map(|l| l.voxels.len()).sum()
    }
}
#[derive(Debug, Clone)]
pub struct Hva {
    pub label: [u8; 16],
    pub sections: Vec<String>,
    pub frames: usize,
    transforms: Vec<Transform>,
}
impl Hva {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 24 || bytes.len() > MAX_FILE {
            return Err("invalid HVA size");
        }
        let frames = u32_at(bytes, 16)? as usize;
        let count = u32_at(bytes, 20)? as usize;
        // Shipped probe.hva is a 24-byte, one-frame, zero-section placeholder.
        // Preserve it; bind() cannot supply a model pose from an empty HVA.
        if frames == 0 || frames > 4096 || count > MAX_SECTIONS {
            return Err("invalid HVA frame/section counts");
        }
        let matrices = frames.checked_mul(count).ok_or("HVA count overflow")?;
        if 24 + 16 * count + 48 * matrices != bytes.len() {
            return Err("HVA body size mismatch");
        }
        let sections = (0..count)
            .map(|i| name(&bytes[24 + i * 16..40 + i * 16]))
            .collect::<Result<Vec<_>, _>>()?;
        let start = 24 + count * 16;
        let transforms = (0..matrices)
            .map(|i| Transform::read(bytes, start + i * 48))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            label: bytes[..16].try_into().unwrap(),
            sections,
            frames,
            transforms,
        })
    }
    pub fn transform(&self, frame: usize, section: usize) -> Option<&Transform> {
        if frame >= self.frames || section >= self.sections.len() {
            return None;
        }
        self.transforms.get(frame * self.sections.len() + section)
    }
    /// Stock readers associate model limbs with HVA sections by index. Some
    /// shipped HTK files have different section names despite matching counts.
    pub fn bind(&self, vxl: &Vxl) -> Result<Vec<usize>, &'static str> {
        if self.sections.len() != vxl.limbs.len() {
            return Err("HVA/VXL section counts differ");
        }
        Ok((0..self.sections.len()).collect())
    }
    /// Optional strict name matching for tooling; not stock engine binding.
    pub fn bind_by_name(&self, vxl: &Vxl) -> Result<Vec<usize>, &'static str> {
        let names = self
            .sections
            .iter()
            .map(|n| n.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let unique = names.iter().collect::<BTreeSet<_>>();
        let limbs = vxl
            .limbs
            .iter()
            .map(|l| l.name.to_ascii_lowercase())
            .collect::<Vec<_>>();
        if names.len() != limbs.len()
            || unique.len() != names.len()
            || limbs.iter().collect::<BTreeSet<_>>().len() != limbs.len()
        {
            return Err("ambiguous or mismatched HVA/VXL sections");
        }
        limbs
            .iter()
            .map(|name| {
                names
                    .iter()
                    .position(|n| n == name)
                    .ok_or("HVA section missing for VXL limb")
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn put(bytes: &mut [u8], at: usize, n: u32) {
        bytes[at..at + 4].copy_from_slice(&n.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 948];
        bytes[..16].copy_from_slice(b"Voxel Animation\0");
        for (at, n) in [
            (16, 1),
            (20, 1),
            (24, 1),
            (28, 26),
            (818, 0),
            (822, 1),
            (826, 0),
            (830, 0),
            (834, u32::MAX),
            (838, 9),
            (842, u32::MAX),
            (856, 0),
            (860, 8),
            (864, 16),
        ] {
            put(&mut bytes, at, n);
        }
        bytes[32..34].copy_from_slice(&[16, 31]);
        bytes[802..806].copy_from_slice(b"Body");
        bytes[846..856].copy_from_slice(&[1, 2, 5, 44, 6, 55, 2, 2, 0, 0]);
        put(&mut bytes, 868, 1.0_f32.to_bits());
        for i in [0, 5, 10] {
            put(&mut bytes, 872 + i * 4, 1.0_f32.to_bits());
        }
        for (at, n) in [(932, 2.0_f32), (936, 1.0), (940, 5.0)] {
            put(&mut bytes, at, n.to_bits());
        }
        bytes[944..948].copy_from_slice(&[2, 1, 5, 4]);
        bytes
    }
    #[test]
    fn sparse_columns_runs_metadata_and_all_truncations() {
        let bytes = fixture();
        let v = Vxl::parse(&bytes).unwrap();
        assert_eq!(v.remap, [16, 31]);
        assert_eq!(v.voxel_count(), 2);
        assert_eq!(
            v.limbs[0].voxels,
            vec![
                Voxel {
                    position: [0, 0, 1],
                    color: 5,
                    normal: 44
                },
                Voxel {
                    position: [0, 0, 2],
                    color: 6,
                    normal: 55
                }
            ]
        );
        assert_eq!(v.limbs[0].normal_mode, 4);
        for end in 0..bytes.len() {
            assert!(Vxl::parse(&bytes[..end]).is_err(), "truncation {end}");
        }
        for (at, n) in [
            (28, u32::MAX),
            (20, 257),
            (856, 26),
            (830, 2),
            (868, f32::NAN.to_bits()),
        ] {
            let mut bad = bytes.clone();
            put(&mut bad, at, n);
            assert!(Vxl::parse(&bad).is_err(), "offset {at}");
        }
        for (at, n) in [(846, 0), (847, 0), (852, 1), (853, 9)] {
            let mut bad = bytes.clone();
            bad[at] = n;
            assert!(Vxl::parse(&bad).is_err());
        }
    }
    #[test]
    fn hva_frame_major_matrices_binding_and_bounds() {
        let mut bytes = vec![0; 24 + 32 + 4 * 48];
        put(&mut bytes, 16, 2);
        put(&mut bytes, 20, 2);
        bytes[24..30].copy_from_slice(b"Turret");
        bytes[40..44].copy_from_slice(b"Body");
        for i in 0..4 {
            for diagonal in [0, 5, 10] {
                put(&mut bytes, 56 + i * 48 + diagonal * 4, 1.0_f32.to_bits());
            }
            put(
                &mut bytes,
                56 + i * 48 + 12,
                ((i / 2 * 10 + i % 2) as f32).to_bits(),
            );
        }
        let h = Hva::parse(&bytes).unwrap();
        assert_eq!(
            h.transform(1, 0).unwrap().apply([2.0, 3.0, 4.0]),
            Ok([12.0, 3.0, 4.0])
        );
        assert!(h.transform(2, 0).is_none());
        assert!(h.transform(0, 2).is_none());
        let mut v = Vxl::parse(&fixture()).unwrap();
        let mut turret = v.limbs[0].clone();
        turret.name = "TURRET".into();
        v.limbs.push(turret);
        assert_eq!(h.bind(&v), Ok(vec![0, 1]));
        assert_eq!(h.bind_by_name(&v), Ok(vec![1, 0]));
        v.limbs[1].name = "Body".into();
        assert!(h.bind_by_name(&v).is_err());
        assert_eq!(h.bind(&v), Ok(vec![0, 1]));
        for end in 0..bytes.len() {
            assert!(Hva::parse(&bytes[..end]).is_err());
        }
        let mut bad = bytes.clone();
        put(&mut bad, 56, f32::INFINITY.to_bits());
        assert!(Hva::parse(&bad).is_err());
        assert!(
            h.transform(0, 0)
                .unwrap()
                .apply([f32::NAN, 0.0, 0.0])
                .is_err()
        );
        let mut empty = vec![0; 24];
        put(&mut empty, 16, 1);
        let empty = Hva::parse(&empty).unwrap();
        assert!(empty.sections.is_empty());
        assert!(empty.transform(0, 0).is_none());
        assert!(empty.bind(&Vxl::parse(&fixture()).unwrap()).is_err());
    }
}

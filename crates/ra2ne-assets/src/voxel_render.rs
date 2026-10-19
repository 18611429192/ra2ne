//! Unlit and normal-lit orthographic voxel previews.
//!
//! The shaded path reproduces the original engine's voxel shading: `FUN_007586f0`
//! walks the normal table selected by the limb's generation byte together with the
//! model-space light direction, keeps `diffuse = max(0, n·L)` and
//! `specular = max(0, n·H / (3 - 2·n·H))` with the half vector `H = normalize(L+U)`
//! against the transformed up vector, and stores `round(16 · (diffuse + specular))`
//! as the section that indexes `voxels.vpl` (draw code at 0x7576b9,
//! `MOV CL,[EDX+EBX+0xb41178]`).
use crate::{
    normals,
    sprite::IndexedImage,
    voxel::{Hva, Vxl},
    vpl::Vpl,
};
pub struct Component<'a> {
    pub model: &'a Vxl,
    pub pose: &'a Hva,
    pub frame: usize,
    pub yaw: f32,
}
pub struct Shading<'a> {
    pub vpl: &'a Vpl,
}
pub struct Raster {
    pub image: IndexedImage,
    pub offset: (i32, i32),
}
/// Section for one voxel normal, mirroring `FUN_007586f0`.
pub fn section(normal: [f32; 3], light: [f32; 3], up: [f32; 3]) -> i32 {
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let mut half = [light[0] + up[0], light[1] + up[1], light[2] + up[2]];
    let length = (half[0] * half[0] + half[1] * half[1] + half[2] * half[2]).sqrt();
    if length != 0.0 {
        half = half.map(|v| v / length);
    }
    let diffuse = dot(normal, light).max(0.0);
    let specular = (dot(normal, half) / (3.0 - 2.0 * dot(normal, half))).max(0.0);
    ((diffuse + specular) * normals::SECTION_SCALE).round() as i32
}
/// Model-space light and up vectors for one limb.
///
/// The preview pipeline maps voxel space to the screen as
/// `R_yaw · flip_y · (scale · T)`, so the inverse rotation used for shading is
/// `Tᵀ · flip_y · R_yaw⁻¹`. `Tᵀ` mirrors the engine's own world-to-model matrix
/// (`FUN_005afc20` transposes the rotation); the engine additionally applies the
/// matrix translation to the light, which is not reproduced here.
pub fn limb_light(yaw: f32, transform: &crate::voxel::Transform) -> ([f32; 3], [f32; 3]) {
    let (s, c) = (-yaw).sin_cos();
    let light = normals::LIGHT_DIRECTION;
    let world = [
        light[0] * c - light[1] * s,
        light[0] * s + light[1] * c,
        light[2],
    ];
    let world_up = [0.0, 0.0, 1.0];
    let to_model = |v: [f32; 3]| {
        let f = [v[0], -v[1], v[2]];
        [
            transform.rows[0][0] * f[0] + transform.rows[1][0] * f[1] + transform.rows[2][0] * f[2],
            transform.rows[0][1] * f[0] + transform.rows[1][1] * f[1] + transform.rows[2][1] * f[2],
            transform.rows[0][2] * f[0] + transform.rows[1][2] * f[1] + transform.rows[2][2] * f[2],
        ]
    };
    (to_model(world), to_model(world_up))
}
fn shade_table(limb: &crate::voxel::Limb, light: [f32; 3], up: [f32; 3]) -> Option<Vec<i32>> {
    let table = normals::table(limb.normal_mode)?;
    Some(table.iter().map(|n| section(*n, light, up)).collect())
}
pub fn render(
    components: &[Component<'_>],
    shading: Option<&Shading<'_>>,
) -> Result<Raster, &'static str> {
    let mut points = Vec::new();
    let mut min = [i32::MAX; 2];
    let mut max = [i32::MIN; 2];
    for component in components {
        if !component.yaw.is_finite() {
            return Err("invalid voxel yaw");
        }
        component.pose.bind(component.model)?;
        let (s, c) = component.yaw.sin_cos();
        for (i, limb) in component.model.limbs.iter().enumerate() {
            let mut transform = *component
                .pose
                .transform(component.frame, i)
                .ok_or("voxel pose frame outside HVA")?;
            let step = std::array::from_fn::<_, 3, _>(|j| {
                (limb.bounds[1][j] - limb.bounds[0][j]) / f32::from(limb.dimensions[j])
            });
            for (j, row) in transform.rows.iter_mut().enumerate() {
                row[3] *= limb.scale * step[j];
            }
            let shades = match shading {
                Some(_) => {
                    let (light, up) = limb_light(component.yaw, &transform);
                    shade_table(limb, light, up)
                }
                None => None,
            };
            for voxel in &limb.voxels {
                if voxel.color == 0 {
                    continue;
                }
                let color = match (&shades, shading) {
                    // The engine reads every normal index straight out of the
                    // generation table; a model using an index past its table would
                    // read unrelated memory, so keep the raw colour there instead.
                    (Some(shades), Some(shading)) => shades
                        .get(usize::from(voxel.normal))
                        .map_or(voxel.color, |section| {
                            shading.vpl.shade(*section, voxel.color)
                        }),
                    _ => voxel.color,
                };
                let p = std::array::from_fn(|j| {
                    limb.bounds[0][j] + (f32::from(voxel.position[j]) + 0.5) * step[j]
                });
                let p = transform.apply(p)?;
                let x = p[0] * limb.scale;
                let y = -p[1] * limb.scale;
                let z = p[2] * limb.scale;
                let (x, y) = (x * c - y * s, x * s + y * c);
                // Preview camera uses twelve pixels per model unit.
                let screen = [
                    (x - y) * 0.70710677 * 12.0,
                    ((x + y) * 0.35355338 - z * 0.8660254) * 12.0,
                ];
                if screen.iter().any(|v| !v.is_finite() || v.abs() > 4096.0) {
                    return Err("voxel preview bounds exceed limits");
                }
                let pixel = screen.map(|v| v.round() as i32);
                for j in 0..2 {
                    min[j] = min[j].min(pixel[j] - 1);
                    max[j] = max[j].max(pixel[j] + 1);
                }
                points.push((pixel, (x + y) * 0.6123724 + z * 0.5, color));
                if points.len() > 4_194_304 {
                    return Err("voxel preview point budget exceeded");
                }
            }
        }
    }
    if points.is_empty() {
        return Err("no visible voxel preview points");
    }
    let width = max[0] - min[0] + 1;
    let height = max[1] - min[1] + 1;
    if width > 1024 || height > 1024 {
        return Err("voxel preview image exceeds 1024x1024");
    }
    let size = (width * height) as usize;
    let mut pixels = vec![0; size];
    let mut depths = vec![f32::NEG_INFINITY; size];
    for (p, depth, color) in points {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let x = p[0] + dx - min[0];
                let y = p[1] + dy - min[1];
                let at = (y * width + x) as usize;
                if depth > depths[at] {
                    depths[at] = depth;
                    pixels[at] = color;
                }
            }
        }
    }
    Ok(Raster {
        image: IndexedImage {
            width: width as u16,
            height: height as u16,
            pixels,
        },
        offset: (min[0], min[1]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::{Limb, Transform, Voxel};
    fn fixture() -> (Vxl, Hva) {
        let identity = Transform {
            rows: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
        };
        let model = Vxl {
            remap: [16, 31],
            palette: [[0; 3]; 256],
            limbs: vec![Limb {
                name: "body".into(),
                index: 0,
                header_flags: [0; 2],
                dimensions: [2, 1, 1],
                scale: 1.,
                transform: identity,
                bounds: [[0., 0., 0.], [2., 1., 1.]],
                normal_mode: 2,
                voxels: vec![
                    Voxel {
                        position: [0, 0, 0],
                        color: 7,
                        normal: 0,
                    },
                    Voxel {
                        position: [1, 0, 0],
                        color: 9,
                        normal: 0,
                    },
                ],
            }],
        };
        let mut bytes = vec![0; 40];
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        bytes[24..28].copy_from_slice(b"body");
        for row in identity.rows {
            for value in row {
                bytes.extend(value.to_le_bytes());
            }
        }
        (model, Hva::parse(&bytes).unwrap())
    }
    /// Sections map colour `c` to `c ^ section` so tests can see the section.
    fn shading() -> Vpl {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&31u32.to_le_bytes());
        bytes.extend_from_slice(&32u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend(std::iter::repeat_n(0u8, 768));
        for s in 0..32u32 {
            for c in 0..256u32 {
                bytes.push((c as u8) ^ (s as u8));
            }
        }
        Vpl::parse(&bytes).unwrap()
    }
    #[test]
    fn composition_depth_is_independent_of_component_order() {
        let (a, pose) = fixture();
        let mut b = a.clone();
        for v in &mut b.limbs[0].voxels {
            v.color = 12;
            v.position[0] += 1;
        }
        b.limbs[0].bounds = [[1., -1., 1.], [3., 0., 2.]];
        let make = |model| Component {
            model,
            pose: &pose,
            frame: 0,
            yaw: 0.,
        };
        let one = render(&[make(&a), make(&b)], None).unwrap();
        let two = render(&[make(&b), make(&a)], None).unwrap();
        assert_eq!(one.image.pixels, two.image.pixels);
        assert_eq!(one.offset, two.offset);
        assert!(one.image.pixels.contains(&12));
    }
    #[test]
    fn invalid_animation_and_empty_models_are_errors() {
        let (model, pose) = fixture();
        assert!(
            render(
                &[Component {
                    model: &model,
                    pose: &pose,
                    frame: 1,
                    yaw: 0.
                }],
                None
            )
            .is_err()
        );
        assert!(
            render(
                &[Component {
                    model: &model,
                    pose: &pose,
                    frame: 0,
                    yaw: f32::NAN
                }],
                None
            )
            .is_err()
        );
        assert!(render(&[], None).is_err());
    }
    #[test]
    fn sections_follow_the_engine_diffuse_and_specular_terms() {
        // Light straight up with the same up vector: a top-facing normal reaches
        // diffuse 1 and specular 1/(3-2) = 1, so 16 * 2 rounds to the last section.
        assert_eq!(section([0., 0., 1.], [0., 0., 1.], [0., 0., 1.]), 32);
        // Side normals see no light and no specular from a straight-up light.
        assert_eq!(section([0., 1., 0.], [0., 0., 1.], [0., 0., 1.]), 0);
        assert_eq!(section([1., 0., 0.], [0., 0., 1.], [0., 0., 1.]), 0);
        // Back-facing normals clamp to zero instead of going negative.
        assert_eq!(section([0., 0., -1.], [0., 0., 1.], [0., 0., 1.]), 0);
        // The engine's own light and up vectors: the top normal is mid-bright.
        let (light, up) = limb_light(
            0.0,
            &Transform {
                rows: [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            },
        );
        // The preview flips voxel Y, so the engine light keeps X and Z and
        // negates Y; a zero yaw leaves the up vector untouched.
        let engine = normals::LIGHT_DIRECTION;
        assert_eq!(light, [engine[0], -engine[1], engine[2]]);
        assert_eq!(up, [0., 0., 1.]);
        // Top normal: diffuse L.z = 0.5 plus the half-vector specular
        // 0.866/(3-1.732) = 0.683 gives (0.5 + 0.683) * 16 = 18.9 -> section 19.
        assert_eq!(section([0., 0., 1.], light, up), 19);
        // The normal facing the light is the brightest: diffuse 1 plus the same
        // half-vector specular 0.683 gives 26.9 -> section 27.
        assert_eq!(section(light, light, up), 27);
    }
    #[test]
    fn shading_uses_vpl_sections_and_falls_back_without_tables() {
        let (model, pose) = fixture();
        let vpl = shading();
        let (light, up) = limb_light(0.0, &model.limbs[0].transform);
        // Use the generation 2 normal that faces the light so the section differs
        // from the raw colour (`fixture` normal 0 points away and stays unshaded).
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let (index, normal) = normals::GENERATION_2
            .iter()
            .enumerate()
            .max_by(|a, b| dot(*a.1, light).partial_cmp(&dot(*b.1, light)).unwrap())
            .unwrap();
        let mut lit = model.clone();
        for voxel in &mut lit.limbs[0].voxels {
            voxel.normal = index as u8;
        }
        let shaded = render(
            &[Component {
                model: &lit,
                pose: &pose,
                frame: 0,
                yaw: 0.,
            }],
            Some(&Shading { vpl: &vpl }),
        )
        .unwrap();
        let plain = render(
            &[Component {
                model: &lit,
                pose: &pose,
                frame: 0,
                yaw: 0.,
            }],
            None,
        )
        .unwrap();
        let section = section(*normal, light, up);
        assert!(section > 0, "section {section}");
        let expected = vpl.shade(section, 7);
        assert_ne!(expected, 7);
        assert_ne!(shaded.image.pixels, plain.image.pixels);
        assert!(shaded.image.pixels.contains(&expected));
        // An unknown generation keeps the raw colours.
        let mut other = lit.clone();
        other.limbs[0].normal_mode = 9;
        let fallback = render(
            &[Component {
                model: &other,
                pose: &pose,
                frame: 0,
                yaw: 0.,
            }],
            Some(&Shading { vpl: &vpl }),
        )
        .unwrap();
        assert_eq!(fallback.image.pixels, plain.image.pixels);
    }
}

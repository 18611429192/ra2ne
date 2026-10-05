//! Basic unlit orthographic voxel previews. Normal lighting/shadows are pending.
use crate::{
    sprite::IndexedImage,
    voxel::{Hva, Vxl},
};
pub struct Component<'a> {
    pub model: &'a Vxl,
    pub pose: &'a Hva,
    pub frame: usize,
    pub yaw: f32,
}
pub struct Raster {
    pub image: IndexedImage,
    pub offset: (i32, i32),
}
pub fn render(components: &[Component<'_>]) -> Result<Raster, &'static str> {
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
            for voxel in &limb.voxels {
                if voxel.color == 0 {
                    continue;
                }
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
                points.push((pixel, (x + y) * 0.6123724 + z * 0.5, voxel.color));
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
        let one = render(&[make(&a), make(&b)]).unwrap();
        let two = render(&[make(&b), make(&a)]).unwrap();
        assert_eq!(one.image.pixels, two.image.pixels);
        assert_eq!(one.offset, two.offset);
        assert!(one.image.pixels.contains(&12));
    }
    #[test]
    fn invalid_animation_and_empty_models_are_errors() {
        let (model, pose) = fixture();
        assert!(
            render(&[Component {
                model: &model,
                pose: &pose,
                frame: 1,
                yaw: 0.
            }])
            .is_err()
        );
        assert!(
            render(&[Component {
                model: &model,
                pose: &pose,
                frame: 0,
                yaw: f32::NAN
            }])
            .is_err()
        );
        assert!(render(&[]).is_err());
    }
}

//! Scripted original-resource showcase; this is not a gameplay simulation.
use macroquad::prelude::*;
use ra2ne_assets::{
    mix::{FilenameHash, MixArchive},
    rules::RuleSet,
    sprite::{Palette, Shp},
    vfs::Vfs,
    voxel::{Hva, Vxl},
    voxel_render::{self, Component},
};
use std::{path::Path, sync::Arc};
fn config() -> Conf {
    Conf {
        window_title: "RA2NE resource animation".into(),
        window_width: 1280,
        window_height: 720,
        window_resizable: false,
        ..Default::default()
    }
}
fn texture(image: &ra2ne_assets::sprite::IndexedImage, palette: &Palette) -> Texture2D {
    let t = Texture2D::from_rgba8(image.width, image.height, &palette.rgba(image, true));
    t.set_filter(FilterMode::Nearest);
    t
}
fn ground(cx: f32, cy: f32) {
    for i in -4..=4 {
        let d = i as f32 * 40.;
        draw_line(
            cx - 160. + d,
            cy - 80. - d / 2.,
            cx + 160. + d,
            cy + 80. - d / 2.,
            1.,
            Color::from_rgba(52, 73, 75, 255),
        );
        draw_line(
            cx - 160. + d,
            cy + 80. + d / 2.,
            cx + 160. + d,
            cy - 80. + d / 2.,
            1.,
            Color::from_rgba(52, 73, 75, 255),
        );
    }
}
#[macroquad::main(config)]
async fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("usage: resource_demo GAME_DIR FRAMES_DIR FRAME_COUNT (1..900)");
        return;
    }
    let n = args[2].parse::<usize>().expect("frame count");
    assert!((1..=900).contains(&n));
    std::fs::create_dir(&args[1]).expect("new frame directory required");
    let mut files = Vfs::default();
    for name in ["local.mix", "localmd.mix", "conquer.mix", "conqmd.mix"] {
        let path = Path::new(&args[0]).join(name);
        let bytes = std::fs::read(path).expect("extracted resource MIX");
        assert!(bytes.len() <= 256 * 1024 * 1024);
        files
            .mount_mix(
                name,
                MixArchive::parse(Arc::from(bytes)).unwrap(),
                FilenameHash::Ra2,
            )
            .unwrap();
    }
    let palette =
        Palette::parse(&std::fs::read(Path::new(&args[0]).join("unittem.pal")).unwrap()).unwrap();
    let mut art = RuleSet::default();
    art.add_layer(
        "art",
        &std::fs::read_to_string(Path::new(&args[0]).join("artmd.ini")).unwrap(),
    )
    .unwrap();
    let mut vehicles = Vec::new();
    for stem in ["htnk", "mtnk"] {
        let mut parts = Vec::new();
        for suffix in ["", "tur", "barl"] {
            let name = format!("{stem}{suffix}");
            if let Some(v) = files.get(&format!("{name}.vxl")).unwrap() {
                let model = Vxl::parse(v.bytes).unwrap();
                let pose = Hva::parse(
                    files
                        .get(&format!("{name}.hva"))
                        .unwrap()
                        .expect("HVA")
                        .bytes,
                )
                .unwrap();
                pose.bind(&model).unwrap();
                parts.push((model, pose));
            } else {
                assert!(!suffix.is_empty(), "vehicle body missing");
            }
        }
        vehicles.push(parts);
    }
    let mut soldiers = Vec::new();
    for name in ["GI", "CONS", "DOG"] {
        let sequence = &art.get(name, "Sequence").unwrap().entry.value;
        let walk = art
            .get(sequence, "Walk")
            .unwrap()
            .entry
            .value
            .split(',')
            .map(|v| v.trim().parse::<usize>().unwrap())
            .collect::<Vec<_>>();
        assert!(walk.len() >= 3 && walk[1] > 0);
        let shp = Shp::parse(Arc::from(
            files.get(&format!("{name}.shp")).unwrap().unwrap().bytes,
        ))
        .unwrap();
        let frames = (0..walk[1])
            .map(|i| texture(&shp.frame(walk[0] + 2 * walk[2] + i).unwrap(), &palette))
            .collect::<Vec<_>>();
        soldiers.push(frames);
    }
    for frame in 0..n {
        clear_background(Color::from_rgba(16, 25, 35, 255));
        draw_text("RA2NE / ORIGINAL RESOURCE ANIMATION", 48., 57., 34., WHITE);
        draw_text(
            "Native Rust renderer | VXL + HVA vehicles / SHP infantry",
            48.,
            92.,
            22.,
            LIGHTGRAY,
        );
        draw_rectangle(36., 124., 1208., 262., Color::from_rgba(25, 39, 48, 255));
        draw_rectangle(36., 402., 1208., 236., Color::from_rgba(25, 39, 48, 255));
        draw_text(
            "VEHICLES / rotating body, turret and barrel",
            58.,
            156.,
            23.,
            SKYBLUE,
        );
        for (i, parts) in vehicles.iter().enumerate() {
            let yaw = frame as f32 * std::f32::consts::TAU / 180.;
            let components = parts
                .iter()
                .enumerate()
                .map(|(j, (model, pose))| Component {
                    model,
                    pose,
                    frame: frame / 3 % pose.frames,
                    yaw: if j == 0 {
                        yaw
                    } else {
                        yaw + 0.45 * (frame as f32 / 30.).sin()
                    },
                })
                .collect::<Vec<_>>();
            let raster = voxel_render::render(&components).unwrap();
            let t = texture(&raster.image, &palette);
            let cx = 350. + i as f32 * 580.;
            let cy = 270.;
            ground(cx, cy);
            draw_texture_ex(
                &t,
                cx + raster.offset.0 as f32 * 3.,
                cy + raster.offset.1 as f32 * 3.,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(t.width() * 3., t.height() * 3.)),
                    ..Default::default()
                },
            );
            draw_text(
                if i == 0 { "RHINO TANK" } else { "GRIZZLY TANK" },
                cx - 75.,
                367.,
                21.,
                WHITE,
            );
        }
        draw_text(
            "INFANTRY / original Walk sequence frames",
            58.,
            435.,
            23.,
            SKYBLUE,
        );
        for (i, frames) in soldiers.iter().enumerate() {
            let cx = 230. + i as f32 * 400.;
            let cy = 545.;
            ground(cx, cy);
            for k in 0..3 {
                let t = &frames[(frame / 3 + k) % frames.len()];
                let x = cx - 110. + k as f32 * 70. + (frame as f32 / 30.).sin() * 24.;
                draw_texture_ex(
                    t,
                    x - t.width() * 1.5,
                    cy - t.height() * 1.5 + (k as f32 - 1.) * 12.,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(t.width() * 3., t.height() * 3.)),
                        ..Default::default()
                    },
                );
            }
            draw_text(
                ["GI", "CONSCRIPT", "ATTACK DOG"][i],
                cx - 55.,
                620.,
                21.,
                WHITE,
            );
        }
        draw_text(
            "Scripted resource demo | Unlit voxels | Combat and pathfinding integration pending",
            48.,
            680.,
            21.,
            YELLOW,
        );
        draw_text(
            format!("{:04} / {:04}", frame + 1, n),
            1080.,
            680.,
            20.,
            GRAY,
        );
        get_screen_data().export_png(&format!("{}/{frame:04}.png", args[1]));
        next_frame().await;
    }
}

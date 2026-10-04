//! Interactive engine preview. Synthetic scenario plus decoded map/sprite views.
//! This is an integration milestone, not the finished RA2/YR game.
mod battle;
use battle::Simulation;
use macroquad::prelude::*;
use ra2ne_assets::{
    map::Ra2Map,
    sprite::{Palette, Shp},
    text::TextEncoding,
};
use ra2ne_core::{Unit, Vec2 as Cell, World, navigation::NavigationMap};
use std::{collections::BTreeSet, io::Read, sync::Arc};

const SIDEBAR: f32 = 264.0;
#[derive(Default)]
struct Options {
    units: usize,
    map: Option<String>,
    sprite: Option<String>,
    palette: Option<String>,
    encoding: Option<TextEncoding>,
    smoke_frames: Option<u64>,
    screenshot: Option<String>,
    headless_ticks: Option<u64>,
    autoplay: bool,
    battle: bool,
    load_game: Option<String>,
    save_game: Option<String>,
}
impl Options {
    fn parse() -> Result<Self, String> {
        let mut options = Self {
            units: 512,
            ..Default::default()
        };
        for arg in std::env::args().skip(1) {
            if arg == "--battle" {
                options.battle = true;
            } else if let Some(v) = arg.strip_prefix("--load-game=") {
                options.load_game = Some(v.into());
            } else if let Some(v) = arg.strip_prefix("--save-game=") {
                options.save_game = Some(v.into());
            } else if arg == "--autoplay" {
                options.autoplay = true;
            } else if let Some(v) = arg.strip_prefix("--units=") {
                options.units = v.parse().map_err(|_| "invalid unit count")?;
            } else if let Some(v) = arg.strip_prefix("--map=") {
                options.map = Some(v.into());
            } else if let Some(v) = arg.strip_prefix("--sprite=") {
                options.sprite = Some(v.into());
            } else if let Some(v) = arg.strip_prefix("--palette=") {
                options.palette = Some(v.into());
            } else if let Some(v) = arg.strip_prefix("--encoding=") {
                options.encoding = Some(TextEncoding::parse(v)?);
            } else if let Some(v) = arg.strip_prefix("--smoke-frames=") {
                options.smoke_frames = Some(v.parse().map_err(|_| "invalid smoke frame count")?);
            } else if let Some(v) = arg.strip_prefix("--headless-ticks=") {
                options.headless_ticks =
                    Some(v.parse().map_err(|_| "invalid headless tick count")?);
            } else if let Some(v) = arg.strip_prefix("--screenshot=") {
                options.screenshot = Some(v.into());
            } else {
                return Err(format!("unknown option: {arg}"));
            }
        }
        if options.map.is_some() && (options.battle || options.load_game.is_some()) {
            return Err("original map gameplay adapter is pending".into());
        }
        if options
            .save_game
            .as_ref()
            .is_some_and(|p| std::path::Path::new(p).exists())
        {
            return Err("save output already exists".into());
        }
        if options.save_game.is_some() && !options.battle && options.load_game.is_none() {
            return Err("game save requires battle mode".into());
        }
        if options.units > 20_000 {
            return Err("preview supports up to 20000 units".into());
        }
        if options.sprite.is_some() != options.palette.is_some() {
            return Err("sprite and palette must be supplied together".into());
        }
        if options.smoke_frames == Some(0)
            || options.smoke_frames.is_some_and(|n| n > 100_000)
            || options.headless_ticks.is_some_and(|n| n > 1_000_000)
        {
            return Err("run length outside preview limits".into());
        }
        if options
            .screenshot
            .as_ref()
            .is_some_and(|p| std::path::Path::new(p).exists())
        {
            return Err("screenshot output already exists".into());
        }
        Ok(options)
    }
}
struct Scene {
    world: Simulation,
    map: NavigationMap,
    tiles: Vec<(Cell, u8)>,
    owners: Vec<u8>,
    name: String,
    map_view_only: bool,
    messages: Vec<String>,
    sprite: Option<(Shp, Palette)>,
    center: Cell,
}
fn read(path: &str, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| format!("{path}: {e}"))?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("preview asset exceeds byte limit".into());
    }
    Ok(bytes)
}
impl Scene {
    fn tick(&mut self) {
        self.world.tick();
        if let Simulation::Battle(game) = &self.world {
            self.owners = (0..game.movement().unit_count())
                .map(|i| {
                    game.entity_at(i)
                        .and_then(|id| game.actor(id))
                        .map_or(255, |a| a.owner as u8)
                })
                .collect();
        }
    }
    fn save(&self, options: &Options) -> Result<(), String> {
        if let Some(path) = &options.save_game {
            let Simulation::Battle(game) = &self.world else {
                return Err("game save requires battle mode".into());
            };
            let bytes = game.save()?;
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .and_then(|mut f| f.write_all(&bytes))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn load(options: &Options) -> Result<Self, String> {
        let sprite = match (&options.sprite, &options.palette) {
            (Some(shp), Some(pal)) => {
                let shp = Shp::parse(Arc::from(read(shp, 64 * 1024 * 1024)?))?;
                shp.frame(0)?;
                Some((shp, Palette::parse(&read(pal, 768)?)?))
            }
            _ => None,
        };
        let (mut map, mut tiles, units, mut owners, mut name, map_view_only, messages, center) =
            if let Some(path) = &options.map {
                let bytes = read(path, 16 * 1024 * 1024)?;
                let text = options
                    .encoding
                    .unwrap_or(TextEncoding::Utf8)
                    .decode(&bytes)?;
                let original = Ra2Map::parse(&text)?;
                let tiles: Vec<_> = original
                    .tiles
                    .iter()
                    .map(|t| {
                        (
                            Cell::new(i32::from(t.cell.x), i32::from(t.cell.y)),
                            t.height,
                        )
                    })
                    .collect();
                let mut map = NavigationMap::new(512, 512);
                for y in 0..512 {
                    for x in 0..512 {
                        map.set_walkable(Cell::new(x, y), false);
                    }
                }
                for &(p, _) in &tiles {
                    map.set_walkable(p, true);
                }
                let first_house = original.objects.first().map(|o| o.house.clone());
                let owners = original
                    .objects
                    .iter()
                    .map(|o| u8::from(Some(&o.house) != first_house.as_ref()))
                    .collect();
                let units = original
                    .objects
                    .iter()
                    .map(|o| {
                        let p = Cell::new(i32::from(o.cell.x), i32::from(o.cell.y));
                        Unit {
                            position: p,
                            goal: p,
                            speed: 1,
                        }
                    })
                    .collect();
                let center = tiles.first().map_or(Cell::new(32, 32), |t| t.0);
                let mut messages =
                    vec!["Map data viewer: original movement/gameplay pending".into()];
                messages.extend(
                    original
                        .diagnostics
                        .iter()
                        .map(|d| format!("{}: {}", d.section, d.message))
                        .take(8),
                );
                (
                    map,
                    tiles,
                    units,
                    owners,
                    original.name,
                    true,
                    messages,
                    center,
                )
            } else {
                let mut map = NavigationMap::new(64, 64);
                let mut tiles = Vec::new();
                for y in 0..64 {
                    for x in 0..64 {
                        let open = x != 32 || (31..=33).contains(&y);
                        map.set_walkable(Cell::new(x, y), open);
                        if open {
                            tiles.push((Cell::new(x, y), 0));
                        }
                    }
                }
                for y in 31..=33 {
                    map.set_capacity(Cell::new(32, y), 4);
                }
                let units = (0..options.units)
                    .map(|id| {
                        let p = Cell::new((id % 24) as i32, ((id / 24) % 60) as i32);
                        Unit {
                            position: p,
                            goal: p,
                            speed: 1,
                        }
                    })
                    .collect();
                (
                    map,
                    tiles,
                    units,
                    vec![0; options.units],
                    "Narrow crossing".into(),
                    false,
                    vec!["Synthetic engine preview / RA2NE 1.0 in development".into()],
                    Cell::new(28, 28),
                )
            };
        // Keep an explicit bounded destination for the synthetic smoke scenario.
        map.set_capacity(Cell::new(50, 32), usize::MAX);
        let mut world = World::from_units(units);
        if options.autoplay && !map_view_only {
            world.move_group(
                &(0..world.unit_count()).collect::<Vec<_>>(),
                &map,
                Cell::new(50, 32),
            )?;
        }
        let mut world = if let Some(path) = &options.load_game {
            let game = ra2ne_game::Skirmish::load(&read(path, 128 * 1024 * 1024)?)?;
            map = game.map().clone();
            let (width, height) = map.dimensions();
            tiles = (0..height)
                .flat_map(|y| (0..width).map(move |x| Cell::new(x as i32, y as i32)))
                .filter(|&p| map.is_traversable(p))
                .map(|p| (p, 0))
                .collect();
            name = "Restored synthetic skirmish".into();
            Simulation::Battle(Box::new(game))
        } else if options.battle {
            name = "Synthetic skirmish".into();
            Simulation::Battle(Box::new(battle::synthetic(map.clone(), options.units)?))
        } else {
            Simulation::Movement(world)
        };
        if options.autoplay
            && options.load_game.is_none()
            && let Simulation::Battle(game) = &mut world
        {
            for owner in 0..2 {
                let ids = game
                    .entities()
                    .filter(|(_, a)| a.owner == owner && a.kind == 0)
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>();
                game.move_units(owner, &ids, Cell::new(if owner == 0 { 30 } else { 34 }, 32))?;
            }
        }
        if let Simulation::Battle(game) = &world {
            owners = (0..game.movement().unit_count())
                .map(|i| {
                    game.entity_at(i)
                        .and_then(|id| game.actor(id))
                        .map_or(255, |a| a.owner as u8)
                })
                .collect();
        }
        Ok(Self {
            world,
            map,
            tiles,
            owners,
            name,
            map_view_only,
            messages,
            sprite,
            center,
        })
    }
}
#[derive(Clone, Copy)]
struct View {
    center: macroquad::math::Vec2,
    zoom: f32,
}
impl View {
    fn screen(&self, p: Cell, height: u8) -> macroquad::math::Vec2 {
        let iso = vec2(
            (p.x - p.y) as f32 * 30.0,
            (p.x + p.y) as f32 * 15.0 - f32::from(height) * 15.0,
        );
        let viewport = vec2((screen_width() - SIDEBAR) / 2.0, screen_height() / 2.0);
        (iso - self.center) * self.zoom + viewport
    }
    fn cell(&self, screen: macroquad::math::Vec2) -> Cell {
        let viewport = vec2((screen_width() - SIDEBAR) / 2.0, screen_height() / 2.0);
        let p = (screen - viewport) / self.zoom + self.center;
        Cell::new(
            ((p.y / 15.0 + p.x / 30.0) / 2.0).round() as i32,
            ((p.y / 15.0 - p.x / 30.0) / 2.0).round() as i32,
        )
    }
}
fn diamond(p: macroquad::math::Vec2, width: f32, height: f32, color: Color) {
    let top = p - vec2(0.0, height / 2.0);
    let right = p + vec2(width / 2.0, 0.0);
    let bottom = p + vec2(0.0, height / 2.0);
    let left = p - vec2(width / 2.0, 0.0);
    draw_triangle(top, right, bottom, color);
    draw_triangle(top, bottom, left, color);
}
fn conf() -> Conf {
    Conf {
        window_title: "RA2NE / Engine Preview".into(),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        sample_count: 1,
        ..Default::default()
    }
}
fn main() {
    let result =
        Options::parse().and_then(|options| Scene::load(&options).map(|scene| (options, scene)));
    match result {
        Err(error) => {
            eprintln!("ra2ne-runtime: {error}");
            std::process::exit(1);
        }
        Ok((options, mut scene)) => {
            if let Some(ticks) = options.headless_ticks {
                for _ in 0..ticks {
                    scene.tick();
                }
                if let Err(error) = scene.save(&options) {
                    eprintln!("ra2ne-runtime: {error}");
                    std::process::exit(1);
                }
                println!(
                    "runtime_headless=true; units={}; ticks={}; state_hash={:016x}",
                    scene.world.unit_count(),
                    scene.world.tick_number(),
                    scene.world.state_hash()
                );
            } else {
                macroquad::Window::from_config(conf(), run(options, scene));
            }
        }
    }
}
async fn run(options: Options, mut scene: Scene) {
    let mut view = View {
        center: vec2(
            (scene.center.x - scene.center.y) as f32 * 30.0,
            (scene.center.x + scene.center.y) as f32 * 15.0,
        ),
        zoom: 0.8,
    };
    let texture = scene.sprite.as_ref().map(|(shp, pal)| {
        let image = shp.frame(0).expect("validated sprite frame");
        let texture = Texture2D::from_rgba8(image.width, image.height, &pal.rgba(&image, true));
        texture.set_filter(FilterMode::Nearest);
        texture
    });
    let mut selected = BTreeSet::new();
    let mut drag = None::<macroquad::math::Vec2>;
    let mut previous_mouse = vec2(0.0, 0.0);
    let mut paused = false;
    let mut grid = false;
    let mut debug = false;
    let mut accumulator = 0.0;
    let mut frames = 0_u64;
    let mut status = String::from("Select units, then right-click a destination");
    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::Space) {
            paused = !paused;
        }
        if is_key_pressed(KeyCode::G) {
            grid = !grid;
        }
        if is_key_pressed(KeyCode::F3) {
            debug = !debug;
        }
        let mouse = vec2(mouse_position().0, mouse_position().1);
        let inside = mouse.x < screen_width() - SIDEBAR && mouse.y > 56.0;
        if inside {
            let (_, wheel) = mouse_wheel();
            if wheel != 0.0 {
                view.zoom = (view.zoom * (1.0 + wheel * 0.12)).clamp(0.15, 2.5);
            }
            if is_mouse_button_down(MouseButton::Middle) {
                view.center -= (mouse - previous_mouse) / view.zoom;
            }
            if is_mouse_button_pressed(MouseButton::Left) {
                drag = Some(mouse);
            }
            if is_mouse_button_pressed(MouseButton::Right) && !selected.is_empty() {
                if scene.map_view_only {
                    status = "Original map is open in data-view mode".into();
                } else {
                    let target = view.cell(mouse);
                    let ids: Vec<_> = selected.iter().copied().collect();
                    status = match scene
                        .world
                        .attack_at(&ids, target)
                        .unwrap_or_else(|| scene.world.move_group(&ids, &scene.map, target))
                    {
                        Ok(()) => format!("Command accepted for {} units", ids.len()),
                        Err(_) => "Destination is outside the map or blocked".into(),
                    };
                }
            }
        }
        if is_mouse_button_released(MouseButton::Left)
            && let Some(start) = drag.take()
        {
            if !is_key_down(KeyCode::LeftShift) && !is_key_down(KeyCode::RightShift) {
                selected.clear();
            }
            let rect = Rect::new(
                start.x.min(mouse.x),
                start.y.min(mouse.y),
                (start.x - mouse.x).abs(),
                (start.y - mouse.y).abs(),
            );
            if rect.w < 5.0 && rect.h < 5.0 {
                let nearest = (0..scene.world.unit_count())
                    .filter(|&id| scene.owners[id] == 0)
                    .map(|id| {
                        (
                            id,
                            view.screen(scene.world.unit(id).unwrap().position, 0)
                                .distance(mouse),
                        )
                    })
                    .filter(|(_, d)| *d < 18.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                if let Some((id, _)) = nearest {
                    selected.insert(id);
                }
            } else {
                for id in 0..scene.world.unit_count() {
                    if scene.owners[id] == 0
                        && rect.contains(view.screen(scene.world.unit(id).unwrap().position, 0))
                    {
                        selected.insert(id);
                    }
                }
            }
        }
        if (is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl))
            && is_key_pressed(KeyCode::A)
        {
            selected = (0..scene.world.unit_count())
                .filter(|&id| scene.owners[id] == 0)
                .collect();
        }
        if is_key_pressed(KeyCode::S) {
            scene
                .world
                .stop_group(&selected.iter().copied().collect::<Vec<_>>())
                .ok();
            status = "Selected units stopped".into();
        }
        if is_key_pressed(KeyCode::B)
            && let Simulation::Battle(game) = &mut scene.world
        {
            let factory = game
                .entities()
                .find(|(_, a)| a.owner == 0 && game.rules().units[a.kind].factory)
                .map(|(id, _)| id);
            status = if let Some(factory) = factory {
                match game.queue_production(0, factory, 0) {
                    Ok(()) => "Tank queued".into(),
                    Err(error) => error.into(),
                }
            } else {
                "No factory".into()
            };
        }
        let pan = 500.0 * get_frame_time() / view.zoom;
        if is_key_down(KeyCode::Left) {
            view.center.x -= pan;
        }
        if is_key_down(KeyCode::Right) {
            view.center.x += pan;
        }
        if is_key_down(KeyCode::Up) {
            view.center.y -= pan;
        }
        if is_key_down(KeyCode::Down) {
            view.center.y += pan;
        }
        previous_mouse = mouse;
        if !paused {
            if options.smoke_frames.is_some() {
                scene.tick();
            } else {
                accumulator += get_frame_time().min(0.25);
                let mut steps = 0;
                while accumulator >= 1.0 / 30.0 && steps < 8 {
                    scene.tick();
                    accumulator -= 1.0 / 30.0;
                    steps += 1;
                }
            }
        }
        selected.retain(|&id| scene.world.alive(id) && scene.owners[id] == 0);
        if let Simulation::Battle(game) = &scene.world {
            for event in game.events() {
                if let ra2ne_game::Event::Destroyed { entity } = event {
                    selected.remove(&(entity.index as usize));
                }
            }
        }
        clear_background(Color::from_rgba(13, 19, 20, 255));
        for &(p, height) in &scene.tiles {
            let pos = view.screen(p, height);
            if pos.x < -80.0
                || pos.x > screen_width() - SIDEBAR + 80.0
                || pos.y < -80.0
                || pos.y > screen_height() + 80.0
            {
                continue;
            }
            let tint = if (p.x + p.y) % 2 == 0 {
                Color::from_rgba(55, 71, 47, 255)
            } else {
                Color::from_rgba(60, 77, 50, 255)
            };
            diamond(pos, 60.0 * view.zoom, 30.0 * view.zoom, tint);
            if let Simulation::Battle(game) = &scene.world
                && game.resource(p) > 0
            {
                draw_circle(pos.x, pos.y, 5.0 * view.zoom, GOLD);
            }
            if grid {
                diamond(
                    pos,
                    57.0 * view.zoom,
                    27.0 * view.zoom,
                    Color::from_rgba(44, 60, 39, 255),
                );
            }
        }
        let mut visible: Vec<_> = (0..scene.world.unit_count())
            .filter_map(|id| {
                if !scene.world.alive(id) {
                    return None;
                }
                let unit = scene.world.unit(id).unwrap();
                let p = view.screen(unit.position, 0);
                (p.x >= -80.0
                    && p.x < screen_width() - SIDEBAR + 80.0
                    && p.y >= -80.0
                    && p.y < screen_height() + 80.0)
                    .then_some((id, p, unit.position.x + unit.position.y))
            })
            .collect();
        visible.sort_by_key(|&(id, _, depth)| (depth, id));
        for (id, p, _) in &visible {
            let tint = if scene.owners[*id] == 0 {
                Color::from_rgba(109, 180, 90, 255)
            } else {
                Color::from_rgba(220, 100, 72, 255)
            };
            if let Simulation::Battle(game) = &scene.world
                && let Some(entity) = game.entity_at(*id)
                && let Some(actor) = game.actor(entity)
            {
                let fraction = actor.health as f32 / game.rules().units[actor.kind].health as f32;
                draw_rectangle(p.x - 10.0, p.y - 20.0, 20.0, 3.0, DARKGRAY);
                draw_rectangle(p.x - 10.0, p.y - 20.0, 20.0 * fraction, 3.0, GREEN);
            }
            if selected.contains(id) {
                draw_ellipse_lines(
                    p.x,
                    p.y + 4.0,
                    14.0 * view.zoom,
                    7.0 * view.zoom,
                    0.0,
                    1.5,
                    Color::from_rgba(190, 236, 147, 255),
                );
            }
            if let Some(texture) = &texture {
                draw_texture_ex(
                    texture,
                    p.x - texture.width() * view.zoom / 2.0,
                    p.y - texture.height() * view.zoom,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(texture.width(), texture.height()) * view.zoom),
                        ..Default::default()
                    },
                );
            } else {
                let scale = view.zoom.max(0.4);
                draw_rectangle(
                    p.x - 9.0 * scale,
                    p.y - 6.0 * scale,
                    18.0 * scale,
                    12.0 * scale,
                    Color::from_rgba(27, 35, 29, 255),
                );
                draw_rectangle(
                    p.x - 7.0 * scale,
                    p.y - 5.0 * scale,
                    14.0 * scale,
                    8.0 * scale,
                    tint,
                );
                draw_circle(p.x, p.y - 2.0 * scale, 4.0 * scale, tint);
                draw_line(
                    p.x,
                    p.y - 2.0 * scale,
                    p.x + 10.0 * scale,
                    p.y - 8.0 * scale,
                    2.0 * scale,
                    tint,
                );
            }
        }
        if let Some(start) = drag {
            draw_rectangle_lines(
                start.x.min(mouse.x),
                start.y.min(mouse.y),
                (start.x - mouse.x).abs(),
                (start.y - mouse.y).abs(),
                1.0,
                Color::from_rgba(193, 222, 162, 255),
            );
        }
        draw_hud(&scene, &selected, &status, paused, debug, visible.len());
        frames += 1;
        if options.smoke_frames.is_some_and(|limit| frames >= limit) {
            if let Some(path) = &options.screenshot {
                get_screen_data().export_png(path);
            }
            println!(
                "runtime_gpu_smoke=true; frames={frames}; ticks={}; units={}; state_hash={:016x}",
                scene.world.tick_number(),
                scene.world.unit_count(),
                scene.world.state_hash()
            );
            break;
        }
        next_frame().await;
    }
    if let Err(error) = scene.save(&options) {
        eprintln!("ra2ne-runtime: {error}");
    }
}
fn draw_hud(
    scene: &Scene,
    selected: &BTreeSet<usize>,
    status: &str,
    paused: bool,
    debug: bool,
    visible: usize,
) {
    let left = screen_width() - SIDEBAR;
    let panel = Color::from_rgba(22, 30, 29, 255);
    let accent = Color::from_rgba(191, 187, 127, 255);
    draw_rectangle(0.0, 0.0, screen_width(), 56.0, panel);
    draw_line(0.0, 55.0, screen_width(), 55.0, 1.0, accent);
    draw_text("RA2NE", 22.0, 36.0, 29.0, accent);
    draw_text(
        &scene.name,
        150.0,
        35.0,
        22.0,
        Color::from_rgba(210, 220, 204, 255),
    );
    if paused {
        draw_text("PAUSED", screen_width() / 2.0, 35.0, 20.0, accent);
    }
    draw_rectangle(left, 56.0, SIDEBAR, screen_height() - 56.0, panel);
    draw_line(left, 56.0, left, screen_height(), 1.0, accent);
    draw_text("COMMAND CENTER", left + 22.0, 94.0, 20.0, accent);
    draw_text(
        format!(
            "{} units / {} selected",
            scene.world.live_count(),
            selected.len()
        ),
        left + 22.0,
        126.0,
        17.0,
        WHITE,
    );
    let minimap = Rect::new(left + 22.0, 150.0, SIDEBAR - 44.0, 150.0);
    draw_rectangle(
        minimap.x,
        minimap.y,
        minimap.w,
        minimap.h,
        Color::from_rgba(42, 53, 38, 255),
    );
    let max = if scene.map_view_only { 512.0 } else { 64.0 };
    for id in 0..scene.world.unit_count() {
        if !scene.world.alive(id) {
            continue;
        }
        let p = scene.world.unit(id).unwrap().position;
        draw_rectangle(
            minimap.x + p.x as f32 / max * minimap.w,
            minimap.y + p.y as f32 / max * minimap.h,
            2.0,
            2.0,
            if scene.owners[id] == 0 { GREEN } else { ORANGE },
        );
    }
    draw_rectangle_lines(minimap.x, minimap.y, minimap.w, minimap.h, 1.0, accent);
    for (index, text) in [
        "Left drag: select",
        "Right click: move",
        "Ctrl+A: select all",
        "S: stop selected",
        "Wheel: zoom",
        "Middle drag / arrows: pan",
        "Space: pause",
        "G: grid   F3: performance",
    ]
    .iter()
    .enumerate()
    {
        draw_text(
            text,
            left + 22.0,
            340.0 + index as f32 * 25.0,
            16.0,
            Color::from_rgba(176, 188, 172, 255),
        );
    }
    draw_text(
        "ENGINE PREVIEW",
        left + 22.0,
        screen_height() - 90.0,
        18.0,
        accent,
    );
    draw_text(
        "Full RA2/YR gameplay pending",
        left + 22.0,
        screen_height() - 64.0,
        15.0,
        Color::from_rgba(151, 163, 149, 255),
    );
    draw_rectangle(
        0.0,
        screen_height() - 36.0,
        left,
        36.0,
        Color::from_rgba(18, 25, 22, 240),
    );
    draw_text(status, 20.0, screen_height() - 13.0, 17.0, WHITE);
    if debug {
        draw_rectangle(15.0, 70.0, 330.0, 105.0, Color::from_rgba(10, 15, 14, 225));
        draw_text(
            format!("FPS {} / visible {}", get_fps(), visible),
            27.0,
            96.0,
            17.0,
            WHITE,
        );
        draw_text(
            format!("Tick {} / 30 TPS", scene.world.tick_number()),
            27.0,
            123.0,
            17.0,
            WHITE,
        );
        draw_text(
            format!("Hash {:016x}", scene.world.state_hash()),
            27.0,
            150.0,
            16.0,
            WHITE,
        );
    }
    if let Simulation::Battle(game) = &scene.world {
        let label = if game.finished() {
            format!("Result: {:?}", game.winner())
        } else {
            format!(
                "Credits {} / power {} / B: build tank",
                game.players().get(&0).map_or(0, |p| p.credits),
                game.power_balance(0)
            )
        };
        draw_text(&label, 22.0, screen_height() - 72.0, 16.0, WHITE);
    }
    if let Some(message) = scene.messages.first() {
        draw_text(
            message,
            22.0,
            screen_height() - 50.0,
            15.0,
            Color::from_rgba(172, 184, 165, 255),
        );
    }
}

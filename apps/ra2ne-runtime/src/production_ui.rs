use macroquad::prelude::*;
use ra2ne_game::{EntityId, Skirmish};
use std::collections::BTreeSet;

pub fn factory(game: &Skirmish, selected: &BTreeSet<usize>) -> Option<EntityId> {
    let valid = |id| {
        game.actor(id)
            .is_some_and(|a| a.owner == 0 && game.rules().units[a.kind].factory)
    };
    selected
        .iter()
        .filter_map(|&i| game.entity_at(i))
        .find(|&id| valid(id))
        .or_else(|| game.entities().map(|(id, _)| id).find(|&id| valid(id)))
}

pub fn draw(game: &mut Skirmish, selected: &BTreeSet<usize>, page: &mut usize) -> Option<String> {
    let x = screen_width() - super::SIDEBAR + 18.0;
    let Some(factory) = factory(game, selected) else {
        draw_text("No owned factory", x, 338.0, 17.0, GRAY);
        return None;
    };
    let actor = game.actor(factory).unwrap();
    draw_text(
        format!("Factory: {}", game.rules().units[actor.kind].name),
        x,
        331.0,
        16.0,
        WHITE,
    );
    let kinds: Vec<_> = game
        .rules()
        .units
        .iter()
        .enumerate()
        .filter(|(_, d)| d.speed > 0)
        .map(|(kind, _)| kind)
        .collect();
    let rows = (((screen_height() - 475.0) / 32.0) as usize).clamp(1, 8);
    let pages = kinds.len().div_ceil(rows).max(1);
    *page = (*page).min(pages - 1);
    let mouse = vec2(mouse_position().0, mouse_position().1);
    let clicked = is_mouse_button_pressed(MouseButton::Left);
    let mut action = None;
    for (row, &kind) in kinds.iter().skip(*page * rows).take(rows).enumerate() {
        let rect = Rect::new(x, 347.0 + row as f32 * 32.0, super::SIDEBAR - 36.0, 29.0);
        let available = game.production_available(0, factory, kind);
        draw_rectangle(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            if available.is_ok() {
                DARKGREEN
            } else {
                DARKGRAY
            },
        );
        let d = &game.rules().units[kind];
        draw_text(
            format!("{}  ${}", d.name, d.cost),
            x + 5.0,
            rect.y + 20.0,
            16.0,
            WHITE,
        );
        if rect.contains(mouse) {
            if let Err(reason) = available {
                draw_text(reason, 20.0, screen_height() - 42.0, 17.0, ORANGE);
            } else if clicked {
                action = Some(
                    game.queue_production(0, factory, kind)
                        .map(|()| "Production queued".to_owned())
                        .unwrap_or_else(str::to_owned),
                );
            }
        }
    }
    let y = 355.0 + rows as f32 * 32.0;
    draw_text(
        format!("Page {}/{}  < / >", *page + 1, pages),
        x,
        y + 15.0,
        16.0,
        WHITE,
    );
    if clicked && Rect::new(x, y, super::SIDEBAR - 36.0, 24.0).contains(mouse) {
        if mouse.x < x + (super::SIDEBAR - 36.0) / 2.0 {
            *page = (*page + pages - 1) % pages;
        } else {
            *page = (*page + 1) % pages;
        }
    }
    if let Some(queue) = game.production(factory) {
        let head = &queue[0];
        draw_text(
            format!("Queue {} / {} ticks", queue.len(), head.remaining),
            x,
            y + 49.0,
            16.0,
            WHITE,
        );
        draw_text("Cancel first (refund)", x, y + 75.0, 16.0, ORANGE);
        if clicked && Rect::new(x, y + 55.0, super::SIDEBAR - 36.0, 26.0).contains(mouse) {
            action = Some(
                game.cancel_production(0, factory, 0)
                    .map(|()| "Production cancelled; credits refunded".to_owned())
                    .unwrap_or_else(str::to_owned),
            );
        }
    }
    if game.power_balance(0) < 0 {
        draw_text("Low power: production paused", x, y + 102.0, 14.0, ORANGE);
    }
    action
}

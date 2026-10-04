use macroquad::prelude::*;
use ra2ne_game::{EntityId, Skirmish, commands::Action};
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

#[derive(Default)]
pub struct State {
    product_page: usize,
    queue_page: usize,
    factory: Option<EntityId>,
}

pub fn draw(game: &Skirmish, selected: &BTreeSet<usize>, state: &mut State) -> Option<Action> {
    let x = screen_width() - super::SIDEBAR + 18.0;
    let Some(factory) = factory(game, selected) else {
        draw_text("No owned factory", x, 338.0, 17.0, GRAY);
        return None;
    };
    if state.factory != Some(factory) {
        state.factory = Some(factory);
        state.product_page = 0;
        state.queue_page = 0;
    }
    let page = &mut state.product_page;
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
    let rows = (((screen_height() - 540.0) / 32.0) as usize).clamp(1, 6);
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
                action = Some(Action::Produce { factory, kind });
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
        let queue_pages = queue.len().div_ceil(3);
        state.queue_page = state.queue_page.min(queue_pages - 1);
        draw_text(
            format!("Queue {} (click to cancel)", queue.len()),
            x,
            y + 49.0,
            15.0,
            WHITE,
        );
        for (row, (index, job)) in queue
            .iter()
            .enumerate()
            .skip(state.queue_page * 3)
            .take(3)
            .enumerate()
        {
            let rect = Rect::new(x, y + 62.0 + row as f32 * 28.0, super::SIDEBAR - 36.0, 25.0);
            draw_rectangle(rect.x, rect.y, rect.w, rect.h, DARKGRAY);
            if index == 0 {
                let total = game.rules().units[job.kind].build_ticks.max(1);
                let progress = total.saturating_sub(job.remaining) as f32 / total as f32;
                draw_rectangle(rect.x, rect.y, rect.w * progress, rect.h, DARKGREEN);
            }
            let name: String = game.rules().units[job.kind].name.chars().take(14).collect();
            draw_text(
                format!("{} {}: {}t", index + 1, name, job.remaining),
                x + 4.0,
                rect.y + 18.0,
                14.0,
                ORANGE,
            );
            if clicked && rect.contains(mouse) {
                action = Some(Action::Cancel { factory, index });
            }
        }
        let rect = Rect::new(x, y + 154.0, super::SIDEBAR - 36.0, 24.0);
        draw_text(
            format!("Queue page {}/{}  < / >", state.queue_page + 1, queue_pages),
            x,
            rect.y + 18.0,
            15.0,
            WHITE,
        );
        if clicked && rect.contains(mouse) {
            if mouse.x < rect.x + rect.w / 2.0 {
                state.queue_page = (state.queue_page + queue_pages - 1) % queue_pages;
            } else {
                state.queue_page = (state.queue_page + 1) % queue_pages;
            }
        }
    } else {
        draw_text("Queue empty", x, y + 49.0, 16.0, GRAY);
    }

    if game.power_balance(0) < 0 {
        draw_text("Low power: production paused", x, y + 194.0, 14.0, ORANGE);
    }
    action
}

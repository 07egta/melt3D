//! Responsive layout rules for the window: how much room the docks may take, which docks a
//! narrow window shows, and where the floating tool cards go so they do not cover the tool strip.
//! Pure functions keep the rules testable without a GPU or an operating-system window.

use egui::{Pos2, Rangef, Rect, pos2};

/// Below this window width only one dock is shown at a time, so the 3-D view keeps its room.
pub const COMPACT_WINDOW: f32 = 960.0;
/// Below this width the status bar keeps only its hint and drops the file name and sizes.
pub const NARROW_STATUS: f32 = 470.0;
/// Below this width the top bar leaves out the mouse / trackpad switch (it stays in the keys card).
pub const NARROW_HEADER: f32 = 900.0;
/// A dock never takes more than this share of the width that is left for it.
const DOCK_SHARE: f32 = 0.54;
/// ...but it never gets narrower than this.
const DOCK_FLOOR: f32 = 160.0;
/// The gap between the tool strip and a floating card.
const CARD_GAP: f32 = 12.0;

/// A window this narrow shows at most one dock.
pub fn compact(window_width: f32) -> bool {
    window_width < COMPACT_WINDOW
}

/// The docks to draw this frame, as (scene list, inspector). A compact window that has both open
/// shows only the inspector; the person's choice is kept, so a wider window shows both again.
pub fn visible_docks(window_width: f32, scene: bool, inspector: bool) -> (bool, bool) {
    if compact(window_width) && scene && inspector { (false, true) } else { (scene, inspector) }
}

/// Open or close one dock, as (scene list, inspector). Opening one in a compact window closes
/// the other, so the click always shows what was asked for.
pub fn toggle_dock(window_width: f32, scene: bool, inspector: bool, which_scene: bool) -> (bool, bool) {
    if which_scene {
        let open = !scene;
        (open, inspector && !(open && compact(window_width)))
    } else {
        let open = !inspector;
        (scene && !(open && compact(window_width)), open)
    }
}

/// The width range a resizable dock may take out of `remaining` pixels: its own `min..=max`,
/// capped so that a small window still has a useful 3-D surface beside it.
pub fn dock_range(remaining: f32, min: f32, max: f32) -> Rangef {
    let remaining = if remaining.is_finite() { remaining.max(0.0) } else { 0.0 };
    let cap = (remaining * DOCK_SHARE).max(DOCK_FLOOR);
    let hi = max.min(cap).max(DOCK_FLOOR);
    let lo = min.min(hi);
    Rangef::new(lo, hi)
}

/// Where a floating card (curve, pipe) goes: the top-left corner of its slot and the slot's width.
/// `bounds` is the visible viewport (already shrunk by the margin), `strip` the tool strip as it
/// was laid out this frame. The card is centred in the viewport when that leaves the strip free;
/// otherwise it moves beside the strip, or below it when even `min_width` does not fit beside.
pub fn card_slot(bounds: Rect, strip: Option<Rect>, max_width: f32, min_width: f32) -> (Pos2, f32) {
    let mut width = bounds.width().clamp(48.0, max_width.max(48.0));
    let mut pos = pos2(bounds.center().x - width * 0.5, bounds.top());
    if let Some(strip) = strip {
        let beside = strip.right() + CARD_GAP;
        if pos.x < beside && strip.intersects(bounds) {
            let room = bounds.right() - beside;
            if room >= min_width {
                width = room.min(max_width);
                pos.x = beside + ((room - width) * 0.5).max(0.0);
            } else {
                pos.y = (strip.bottom() + CARD_GAP).min(bounds.bottom() - 64.0).max(bounds.top());
            }
        }
    }
    (pos, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compact_window_never_shows_two_docks() {
        for width in [320.0, 480.0, 800.0, 959.0] {
            assert_eq!(visible_docks(width, true, true), (false, true));
            assert_eq!(visible_docks(width, true, false), (true, false));
            assert_eq!(visible_docks(width, false, false), (false, false));
        }
        assert_eq!(visible_docks(960.0, true, true), (true, true));
    }

    #[test]
    fn opening_a_dock_in_a_compact_window_closes_the_other() {
        let (s, i) = toggle_dock(480.0, false, true, true);
        assert!(s && !i);
        let (s, i) = toggle_dock(480.0, s, i, false);
        assert!(!s && i);
        let (s, i) = toggle_dock(480.0, s, i, false);
        assert!(!s && !i);
        // a wide window keeps both
        assert_eq!(toggle_dock(1280.0, false, true, true), (true, true));
    }

    #[test]
    fn docks_leave_room_for_the_model() {
        for width in [320.0, 480.0, 800.0, 959.0] {
            let r = dock_range(width, 280.0, 600.0);
            assert!(r.min <= r.max);
            assert!(width - r.max >= width * 0.45 || r.max <= DOCK_FLOOR, "{width}: {r:?}");
        }
        let wide = dock_range(1920.0, 220.0, 560.0);
        assert_eq!((wide.min, wide.max), (220.0, 560.0));
        let odd = dock_range(f32::NAN, 220.0, 560.0);
        assert!(odd.min <= odd.max && odd.max.is_finite());
    }

    #[test]
    fn a_card_moves_beside_the_tool_strip_instead_of_covering_it() {
        let strip = Rect::from_min_size(pos2(12.0, 12.0), egui::vec2(96.0, 400.0));
        for width in [430.0, 640.0, 900.0] {
            let bounds = Rect::from_min_size(pos2(12.0, 12.0), egui::vec2(width, 500.0));
            let (pos, w) = card_slot(bounds, Some(strip), 430.0, 220.0);
            assert!(pos.x >= strip.right() + CARD_GAP - 0.01 || pos.y > strip.bottom(), "{width}: {pos:?}");
            assert!(pos.x + w <= bounds.right() + 0.01, "{width}: {pos:?} {w}");
        }
        // too narrow to sit beside: it goes below the strip
        let bounds = Rect::from_min_size(pos2(12.0, 12.0), egui::vec2(250.0, 900.0));
        let (pos, _) = card_slot(bounds, Some(strip), 430.0, 220.0);
        assert!(pos.y > strip.bottom());
        // a wide view keeps the card centred
        let bounds = Rect::from_min_size(pos2(12.0, 12.0), egui::vec2(1600.0, 900.0));
        let (pos, w) = card_slot(bounds, Some(strip), 430.0, 220.0);
        assert!((pos.x + w * 0.5 - bounds.center().x).abs() < 0.01);
    }
}

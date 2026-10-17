//! Responsive UI rules for YuraCAD. These values describe docked chrome, not floating tools.
//! Pure functions keep size rules testable without a GPU or an operating-system window.

pub const CLASSIC_DOCK: f32 = 272.0;
pub const CLASSIC_INSPECTOR: f32 = 312.0;
pub const SIMPLE_DOCK: f32 = 244.0;
pub const SIMPLE_INSPECTOR: f32 = 276.0;
pub const COMPACT_WINDOW: f32 = 960.0;
pub const TINY_HEADER: f32 = 590.0;
pub const COMPACT_TOOLS: f32 = 650.0;

/// At compact sizes only one dock can be visible; the user can always close it.
pub fn compact(window_width: f32) -> bool {
    window_width < COMPACT_WINDOW
}

/// Resize a requested dock so a small window still has a useful 3-D surface.
pub fn dock_width(remaining_width: f32, preferred: f32) -> f32 {
    preferred.min((remaining_width * 0.54).max(160.0))
}

/// The amount of horizontal space left for the central 3-D viewport, excluding
/// its optional floating tools (which can be moved/closed independently).
pub fn viewport_width(window_width: f32, left: bool, right: bool, simple: bool) -> f32 {
    let mut remaining = window_width;
    if left {
        remaining -= dock_width(remaining, if simple { SIMPLE_DOCK } else { CLASSIC_DOCK });
    }
    if right {
        remaining -= dock_width(remaining, if simple { SIMPLE_INSPECTOR } else { CLASSIC_INSPECTOR });
    }
    remaining.max(0.0)
}

/// Horizontal center and width of a floating drawing card, relative to the viewport.
/// Leave room for the left tool strip instead of covering its buttons on narrow screens.
pub fn floating_card_area(view_width: f32, beginner: bool, max_width: f32) -> (f32, f32) {
    // At very small widths the modelling card temporarily takes priority over
    // the tool rail, which is hidden while drawing (see app::tool_strip).
    let left = if view_width < 260.0 { 4.0 }
        else if beginner {
            if view_width < 430.0 { 105.0 } else { 144.0 }
        } else { 62.0 };
    let right = 12.0;
    let free = (view_width - left - right).max(60.0);
    (left + free * 0.5, free.min(max_width))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beginner_layout_gives_the_viewport_more_width_than_the_original() {
        for width in [960.0, 1280.0, 1440.0, 1920.0] {
            let before = width - CLASSIC_DOCK - CLASSIC_INSPECTOR;
            let novice = viewport_width(width, false, false, true);
            let editing = viewport_width(width, false, true, true);
            assert!(novice > before && editing > before, "{width}: {novice} {editing} {before}");
        }
        assert_eq!(viewport_width(1280.0, false, false, true), 1280.0);
        assert_eq!(viewport_width(1280.0, false, true, true), 1004.0);
    }

    #[test]
    fn small_windows_leave_space_for_a_model_even_with_one_dock_open() {
        for width in [320.0, 480.0, 590.0, 800.0, 959.0] {
            assert!(compact(width));
            for left in [false, true] {
                for right in [false, true] {
                    if left && right { continue; }
                    let actual = viewport_width(width, left, right, true);
                    assert!(actual >= width * 0.45, "{width} {left} {right}: {actual}");
                }
            }
        }
        assert!(!compact(960.0));
    }

    #[test]
    fn drawing_cards_do_not_cover_the_left_toolbar_in_common_small_windows() {
        let (center, card_width) = floating_card_area(180.0, true, 440.0);
        assert!((center - 90.0).abs() < 0.1 && card_width < 180.0);
        for width in [320.0, 430.0, 640.0, 900.0] {
            let (center, card_width) = floating_card_area(width, true, 440.0);
            let card_left = center - card_width / 2.0;
            assert!(card_left >= if width < 430.0 { 105.0 } else { 144.0 });
            assert!(center + card_width * 0.5 <= width - 11.0);
        }
    }

    #[test]
    fn narrow_header_and_bottom_controls_switch_before_overflow() {
        assert!(480.0 < TINY_HEADER && 480.0 < COMPACT_TOOLS);
        assert!(TINY_HEADER < COMPACT_WINDOW);
        assert!(COMPACT_TOOLS < COMPACT_WINDOW);
    }
}

//! Scandinavian light theme tokens — Euler installer.
//! Canvas #FFFFFF, ink #000000 with alpha scale, 8pt spacing, 12px radius.

#![allow(dead_code)]

/// Canvas background — #FFFFFF
pub const CANVAS: &str = "#FFFFFF";
/// Surface background — #FFFFFF (same as canvas for light theme)
pub const SURFACE: &str = "#FFFFFF";
/// Ink base — #000000
pub const INK: &str = "#000000";

/// Alpha scale for ink on light canvas
pub const INK_87: &str = "rgba(0,0,0,0.87)";
pub const INK_60: &str = "rgba(0,0,0,0.60)";
pub const INK_38: &str = "rgba(0,0,0,0.38)";
pub const INK_12: &str = "rgba(0,0,0,0.12)";
pub const INK_06: &str = "rgba(0,0,0,0.06)";
pub const INK_04: &str = "rgba(0,0,0,0.04)";

/// Alpha numeric values (for programmatic use)
pub const INK_87_ALPHA: f32 = 0.87;
pub const INK_60_ALPHA: f32 = 0.60;
pub const INK_38_ALPHA: f32 = 0.38;
pub const INK_12_ALPHA: f32 = 0.12;
pub const INK_06_ALPHA: f32 = 0.06;
pub const INK_04_ALPHA: f32 = 0.04;

/// Interaction states — hover 5% / pressed 9% (light canvas).
/// Light: rgba(0,0,0,0.05) / rgba(0,0,0,0.09)
/// Dark solid fallback on #0a0a0f: #14141a (5% white) / #232326 (9% white)
pub const HOVER_ALPHA: f32 = 0.05;
pub const HOVER: &str = "rgba(0,0,0,0.05)";
/// Solid hex fallback for dark canvas #0a0a0f +5% white overlay (≈ #14141a)
pub const HOVER_HEX: &str = "#14141a";
pub const PRESSED_ALPHA: f32 = 0.09;
pub const PRESSED: &str = "rgba(0,0,0,0.09)";
/// Solid hex fallback for dark canvas #0a0a0f +9% white overlay (≈ #232326)
pub const PRESSED_HEX: &str = "#232326";

/// Disabled / muted — ink 38% text + warm exceeds_limit bar #1a1510
pub const DISABLED_FG: &str = INK_38;
pub const WARM_EXCEEDS_BG: &str = "#1a1510";
pub const WARM_EXCEEDS_BG_ALPHA: &str = "rgba(26,21,16,0.08)";

/// Motion — 150ms ease, disabled when prefers_reduced_motion()
pub const MOTION_DURATION_MS: u32 = 150;
pub const MOTION_EASING: &str = "ease";

/// Focus ring — double: 2px white + 4px ink87 (outer total 6px)
pub const FOCUS_RING: &str = "0 0 0 2px #FFFFFF, 0 0 0 6px rgba(0,0,0,0.87)";
pub const FOCUS_RING_INNER: &str = "0 0 0 2px #FFFFFF";
pub const FOCUS_RING_OUTER: &str = "0 0 0 4px rgba(0,0,0,0.87)";

/// Radius 12px
pub const RADIUS: u32 = 12;
pub const RADIUS_PX: &str = "12px";

/// Shadow 0 8 32 0.08
pub const SHADOW: &str = "0 8px 32px rgba(0,0,0,0.08)";

/// Base spacing unit 8px
pub const SPACING: u32 = 8;

/// Margins (px): mobile 24, tablet 40, desktop 64
pub const MARGIN_MOBILE_PX: u32 = 24;
pub const MARGIN_TABLET_PX: u32 = 40;
pub const MARGIN_DESKTOP_PX: u32 = 64;

/// Section gaps (px): 96 mobile / 144 desktop
pub const SECTION_GAP_MOBILE_PX: u32 = 96;
pub const SECTION_GAP_DESKTOP_PX: u32 = 144;

/// Breakpoints (px)
pub const BP_MOBILE_MAX: usize = 600;
pub const BP_TABLET_MAX: usize = 1024;

/// Font stack — Inter Variable primary, Noto Sans fallback
/// Scandinavian type: single sans family, no display serif.
pub const FONT_SANS: &str = "\"Inter Variable\", \"Noto Sans\", sans-serif";

/// Typography scale — Scandinavian minimal, left-aligned, tabular-nums for numbers.
/// H1 32/40 weight 500 · H2 20/28 weight 500 · BODY 14/20 weight 400 · CAPTION 12/16 ink 38%
pub const H1_SIZE_PX: u32 = 32;
pub const H1_LINE_PX: u32 = 40;
pub const H1_WEIGHT: u32 = 500;
pub const H2_SIZE_PX: u32 = 20;
pub const H2_LINE_PX: u32 = 28;
pub const H2_WEIGHT: u32 = 500;
pub const BODY_SIZE_PX: u32 = 14;
pub const BODY_LINE_PX: u32 = 20;
pub const BODY_WEIGHT: u32 = 400;
pub const CAPTION_SIZE_PX: u32 = 12;
pub const CAPTION_LINE_PX: u32 = 16;
pub const CAPTION_ALPHA: f32 = 0.38; // ink 38% — muted secondary, also used for tabular-nums captions
/// tabular-nums: use `font-variant-numeric: tabular-nums` (or `font-feature-settings: "tnum"`) for
/// numeric columns/badges so digits align vertically; terminal equivalent is right-align fixed width.
/// Measure — prose line length 55–68 chars (ideal ~66ch) for optimal readability (Bringhurst).
/// Renderers should wrap or clamp descriptive text to this measure; never center.
pub const MEASURE_MIN: usize = 55;
pub const MEASURE_MAX: usize = 68;
pub const MEASURE_IDEAL: usize = 66;

/// Margin in terminal chars (maps 24/40/64 px → 3/4/8 chars).
/// 24px≈3 chars, 40px≈4 chars, 64px≈8 chars (8px per char approx; 40px ≈4 at 10px/char).
#[inline]
pub fn margin(width: usize) -> usize {
    if width < 600 {
        3
    } else if width < 1024 {
        4
    } else {
        8
    }
}

/// Section gap in terminal lines.
/// 96px mobile → 3 lines, 144px desktop → 5 lines (approx).
#[inline]
pub fn section_gap(width: usize) -> usize {
    if width < 600 {
        3
    } else {
        5
    }
}

/// Respects `EULER_REDUCED_MOTION` or `NO_ANIM` env vars (any value = reduced).
#[inline]
pub fn prefers_reduced_motion() -> bool {
    std::env::var("EULER_REDUCED_MOTION").is_ok() || std::env::var("NO_ANIM").is_ok()
}

/// Unicode-safe truncation with single glyph `…` (U+2026).
/// - if `s.chars().count() <= max` → `s` unchanged
/// - else if `max <= 1` → `s.chars().take(max)` (0 or 1 char, no ellipsis room)
/// - else → `s.chars().take(max-1) + "…"`
#[inline]
pub fn truncate_str(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    if max <= 1 {
        return s.chars().take(max).collect();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_ascii() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 5), "hell…");
        assert_eq!(truncate_str("hello", 5), "hello");
        assert_eq!(truncate_str("hello", 1), "h");
        assert_eq!(truncate_str("hello", 0), "");
        assert_eq!(truncate_str("", 5), "");
        assert_eq!(truncate_str("ab", 1), "a");
    }

    #[test]
    fn truncate_unicode() {
        // emoji / multi-byte
        assert_eq!(truncate_str("héllo 🌍 world", 7), "héllo …");
        assert_eq!(truncate_str("🦀🦀🦀🦀", 3), "🦀🦀…");
        assert_eq!(truncate_str("🦀🦀", 1), "🦀");
        assert_eq!(truncate_str("🦀🦀", 0), "");
        // width 0,1 edge
        assert_eq!(truncate_str("abc", 0), "");
        assert_eq!(truncate_str("abc", 1), "a");
        // exact length no truncation
        assert_eq!(truncate_str("🦀🦀", 2), "🦀🦀");
    }

    #[test]
    fn truncate_very_long() {
        let s = "a".repeat(1000);
        let t = truncate_str(&s, 10);
        assert_eq!(t.chars().count(), 10);
        assert!(t.ends_with('…'));
        let t2 = truncate_str(&s, 1);
        assert_eq!(t2, "a");
    }

    #[test]
    fn margin_breakpoints() {
        assert_eq!(margin(0), 3);
        assert_eq!(margin(599), 3);
        assert_eq!(margin(600), 4);
        assert_eq!(margin(1023), 4);
        assert_eq!(margin(1024), 8);
        assert_eq!(margin(2000), 8);
    }

    #[test]
    fn section_gap_breakpoints() {
        assert_eq!(section_gap(0), 3);
        assert_eq!(section_gap(599), 3);
        assert_eq!(section_gap(600), 5);
        assert_eq!(section_gap(2000), 5);
    }

    #[test]
    fn prefers_reduced_motion_env() {
        // ensure function is callable and respects env; no env set by default
        // we test by setting env var temporarily
        std::env::remove_var("EULER_REDUCED_MOTION");
        std::env::remove_var("NO_ANIM");
        assert!(!prefers_reduced_motion());
        std::env::set_var("EULER_REDUCED_MOTION", "1");
        assert!(prefers_reduced_motion());
        std::env::remove_var("EULER_REDUCED_MOTION");
        std::env::set_var("NO_ANIM", "1");
        assert!(prefers_reduced_motion());
        std::env::remove_var("NO_ANIM");
        assert!(!prefers_reduced_motion());
    }

    #[test]
    fn tokens_sanity() {
        assert_eq!(CANVAS, "#FFFFFF");
        assert_eq!(SURFACE, "#FFFFFF");
        assert_eq!(INK, "#000000");
        assert_eq!(RADIUS, 12);
        assert_eq!(SPACING, 8);
        assert_eq!(MARGIN_MOBILE_PX, 24);
        assert_eq!(MARGIN_TABLET_PX, 40);
        assert_eq!(MARGIN_DESKTOP_PX, 64);
        assert_eq!(SECTION_GAP_MOBILE_PX, 96);
        assert_eq!(SECTION_GAP_DESKTOP_PX, 144);
    }
}

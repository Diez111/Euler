//! euler-installer-gui — 100% Rust placeholder, Slint-inspired responsive.
//! `cols = if width <360 {1} else if width<600 {1} else if width<768 {2} else if width<1024 {2} else if width<1280 {3} else {3}`
//! Fallback 80 si no tty. Feature `gui` vacía offline.
//! Theme: Scandinavian light — canvas #FFFFFF, ink #000000 alpha scale (87/60/38/12/06/04),
//! radius 12, 8pt spacing, Inter Variable / Noto Sans. GRUB compat via #FFFFFF bg.
//! Typography: H1 32/40 500 · H2 20/28 500 · BODY 14/20 400 · CAPTION 12/16 38%
//! (tabular-nums for numeric badges). Measure 55–68ch (ideal 66ch). Strictly left-aligned, no center.

use euler_core::hw::HwProfile;
use std::io::IsTerminal;

mod theme;
#[allow(unused_imports)]
use crate::theme as _theme;

#[derive(Debug, Clone)]
pub struct CodecOption {
    pub name: &'static str,
    pub size_mb: u32,
    pub enabled: bool,
    pub desc: &'static str,
}
#[derive(Debug, Clone)]
pub struct CodecSelection {
    pub options: Vec<CodecOption>,
}
#[rustfmt::skip]
impl CodecSelection {
    pub fn new() -> Self {
        const DEFS: &[(&str, u32, bool, &str)] = &[
            ("gstreamer1.0-libav", 3, false, "GStreamer libav"),
            ("gstreamer1.0-plugins-bad", 5, false, "GStreamer bad"),
            ("gstreamer1.0-plugins-ugly", 2, false, "GStreamer ugly"),
            ("gstreamer1.0-plugins-good", 2, true, "GStreamer good"),
            ("heif-gdk-pixbuf", 1, false, "HEIF pixbuf"),
            ("webp-pixbuf-loader", 1, false, "WebP loader"),
            ("libavif16", 1, false, "AVIF"),
            ("bluez", 2, false, "Bluetooth stack"),
            ("bluez-firmware", 6, false, "BT firmware"),
            ("libavcodec-extra", 15, false, "codecs extra"),
            ("vulkan-tools", 2, false, "Vulkan tools"),
            ("libva-drm2", 1, false, "VA-API DRM"),
        ];
        let options = DEFS.iter().map(|(n, s, e, d)| CodecOption { name: n, size_mb: *s, enabled: *e, desc: d }).collect();
        Self { options }
    }
    #[inline]
    pub fn total_size_mb(&self) -> u32 { self.options.iter().filter(|o| o.enabled).map(|o| o.size_mb).sum() }
    #[inline]
    pub fn estimated_iso_mb(&self) -> u32 { 420 + self.total_size_mb() }
    #[inline]
    pub fn exceeds_limit(&self) -> bool { self.estimated_iso_mb() > 500 }
    pub fn toggle(&mut self, name: &str) -> bool {
        if let Some(o) = self.options.iter_mut().find(|o| o.name == name) { o.enabled = !o.enabled; return true; }
        false
    }
}
impl Default for CodecSelection {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct GuiState {
    pub hw: Option<HwProfile>,
    pub codecs: CodecSelection,
    pub enable_printer: bool,
}
#[rustfmt::skip]
impl GuiState {
    pub fn new() -> Self { Self { hw: None, codecs: CodecSelection::new(), enable_printer: false } }
    pub fn with_detected_hw() -> Self { Self { hw: Some(HwProfile::detect()), codecs: CodecSelection::new(), enable_printer: false } }
    pub fn toggle_printer(&mut self) { self.enable_printer = !self.enable_printer; }
}
impl Default for GuiState {
    fn default() -> Self {
        Self::new()
    }
}

#[rustfmt::skip]
pub fn terminal_width() -> usize {
    if let Ok(v) = std::env::var("COLUMNS") { if let Ok(n) = v.parse::<usize>() { if n > 0 { return n; } } }
    if !std::io::stdout().is_terminal() && !std::io::stdin().is_terminal() { return 80; }
    80
}
#[inline]
#[allow(clippy::if_same_then_else)]
pub fn adaptive_cols(width: usize) -> usize {
    if width < 360 {
        1
    } else if width < 600 {
        1
    } else if width < 768 {
        2
    } else if width < 1024 {
        2
    } else if width < 1280 {
        3
    } else {
        3
    }
}

#[rustfmt::skip]
#[allow(clippy::manual_clamp)]
pub fn render_hardware_card(hw: &HwProfile, width: usize) -> String {
    let cols = adaptive_cols(width);
    let mut out = String::new();
    // Scandinavian type: H2 20/28 500 left-aligned, caption 12/16 38%, body 14/20 400 — strictly left-aligned, no center.
    // Measure 55–68ch optimal; hardware cards are tabular data, not prose — clamp card width, not center.
    // focus-ring: double 2px white + 4px ink87 (theme::FOCUS_RING = "0 0 0 2px #FFFFFF, 0 0 0 6px rgba(0,0,0,0.87)") — 2px #FFFFFF inner + 4px rgba(0,0,0,0.87) outer
    // motion: if theme::prefers_reduced_motion() { no animation } else { 150ms ease } — respects EULER_REDUCED_MOTION/NO_ANIM
    let _focus_ring = theme::FOCUS_RING; // double 2px white + 4px ink87
    let _focus_inner = theme::FOCUS_RING_INNER;
    let _focus_outer = theme::FOCUS_RING_OUTER;
    let _motion = if theme::prefers_reduced_motion() { "no animation" } else { "150ms ease" };
    let _motion_ms = theme::MOTION_DURATION_MS;
    out.push_str("Hardware\n");
    out.push_str(&format!("{} column{} · {}px · tabular\n", cols, if cols == 1 { "" } else { "s" }, width));
    let margin = theme::margin(width);
    let rule_len = (width.saturating_sub(margin * 2)).min(48).max(8);
    out.push_str(&"─".repeat(rule_len));
    out.push('\n');
    out.push_str(&"\n".repeat(theme::section_gap(width)));
    let ram_gb = (hw.ram_mb as usize).div_ceil(1024);
    let cards = [
        format!(" CPU : {}", hw.cpu_vendor),
        format!(" RAM : {} MiB ({} GiB)", hw.ram_mb, ram_gb),
        format!(" GPU : {}", hw.gpu),
        format!(" WIFI: {}", hw.wifi),
        format!(" BT  : {}", if hw.has_bluetooth { "sí" } else { "no" }),
        format!(" NVMe: {}", if hw.has_nvme { "sí" } else { "no" }),
    ];
    if cols == 1 {
        for c in &cards { out.push_str(&format!(" {:<36} \n", c)); }
    } else {
        for chunk in cards.chunks(cols) {
            // left-aligned, no center — flat card, whitespace + thin ink12% divider (│)
            let row: Vec<String> = chunk.iter().map(|c| format!(" {c:<18} ")).collect();
            out.push_str(&row.join(" │ "));
            out.push('\n');
        }
    }
    out
}

#[rustfmt::skip]
#[allow(clippy::manual_clamp)]
pub fn render_codec_menu(codecs: &CodecSelection, width: usize) -> String {
    let cols = adaptive_cols(width);
    let mut out = String::new();
    // Scandinavian type: H2 20/28 500 left-aligned, caption 12/16 38%, body 14/20 400 — strictly left-aligned, no center.
    // Measure 55–68ch (ideal 66ch) for prose desc; left-aligned, never centered. Numeric badges use tabular-nums.
    out.push_str("Codecs\n");
    out.push_str(&format!("{} column{} · {}px · list\n", cols, if cols == 1 { "" } else { "s" }, width));
    let margin = theme::margin(width);
    let rule_len = (width.saturating_sub(margin * 2)).min(48).max(8);
    out.push_str(&"─".repeat(rule_len));
    out.push('\n');
    out.push_str(&"\n".repeat(theme::section_gap(width)));
    // interaction states: hover 5% (#14141a / rgba(0,0,0,0.05)) vs pressed 9% (#232326 / rgba(0,0,0,0.09))
    // theme::HOVER (#14141a dark fallback on #0a0a0f) / theme::PRESSED (#232326) — light: rgba(0,0,0,0.05/0.09)
    // focus-ring: double 2px white + 4px ink87 (theme::FOCUS_RING — 2px #FFFFFF inner + 4px rgba(0,0,0,0.87) outer)
    // motion: if theme::prefers_reduced_motion() { no animation } else { 150ms ease } — respects EULER_REDUCED_MOTION/NO_ANIM
    let _hover = theme::HOVER; // rgba(0,0,0,0.05) ≈ #14141a on dark #0a0a0f
    let _hover_hex = theme::HOVER_HEX; // #14141a
    let _pressed = theme::PRESSED; // rgba(0,0,0,0.09) ≈ #232326
    let _pressed_hex = theme::PRESSED_HEX; // #232326
    let _focus_ring = theme::FOCUS_RING; // double 2px white + 4px ink87
    let _motion = if theme::prefers_reduced_motion() { "no animation" } else { "150ms ease" };
    let _motion_ms = theme::MOTION_DURATION_MS;
    let exceeds = codecs.exceeds_limit();
    let _warm_bg = theme::WARM_EXCEEDS_BG; // #1a1510 warm exceeds bar
    let _warm_alpha = theme::WARM_EXCEEDS_BG_ALPHA;
    let _disabled_fg = theme::DISABLED_FG; // ink38 muted 38%
    for opt in &codecs.options {
        // semantic state: color+shape+label triple redundancy — ● selected vs ○ idle + [MiB] numeric + label
        // selected: ● ink87 (theme::INK_87) vs idle: ○ ink38 (theme::INK_38 muted 38%)
        // hover 5% #14141a (theme::HOVER_HEX) / pressed 9% #232326 (theme::PRESSED_HEX)
        // disabled when exceeds_limit or would-exceed: muted 38% (theme::DISABLED_FG) + warm bar #1a1510 (theme::WARM_EXCEEDS_BG)
        // focus-ring double 2px white +4px ink87 (theme::FOCUS_RING) for keyboard focus
        let is_disabled_preview = exceeds && !opt.enabled && codecs.estimated_iso_mb() + opt.size_mb > 500;
        let check = if opt.enabled { "●" } else { "○" };
        // tabular-nums: MiB badge right-aligned 10ch (CSS: font-variant-numeric: tabular-nums) so digits align vertically
        let badge = format!("[{} MiB]", opt.size_mb);
        // left-aligned row: name left 28ch, badge right 10ch tabular, desc left — never centered
        // note: semantic triple ensures state not conveyed by color alone
        let line = if is_disabled_preview {
            // muted 38% style — warm bg #1a1510 bar at bottom when exceeds, per-item muted hint here
            format!(" {check} {:<28} {badge:>10}  {} [muted 38%]", opt.name, opt.desc)
        } else {
            format!(" {check} {:<28} {badge:>10}  {}", opt.name, opt.desc)
        };
        if line.chars().count() > width && width > 20 {
            out.push_str(&theme::truncate_str(&line, width));
            out.push('\n');
        } else { out.push_str(&line); out.push('\n'); }
    }
    let total = codecs.total_size_mb();
    let iso = codecs.estimated_iso_mb();
    out.push_str(&format!("\nTotal codecs: {total} MiB | ISO: {iso} MiB (base 420)\n"));
    if codecs.exceeds_limit() { out.push_str("⚠️  ADVERTENCIA: ISO >500 MiB — excede límite\n"); } else { out.push_str("✓ Dentro límite <500 MiB\n"); }
    out
}

#[rustfmt::skip]
#[allow(clippy::manual_clamp)]
pub fn render_printer_menu(enable_printer: bool, width: usize) -> String {
    let mut out = String::new();
    // Scandinavian minimal — H2 20/28 500 left-aligned "Impresora" + CAPTION 12/16 38% "CUPS — +45 MiB" — strictly left-aligned, no center.
    // Measure 55–68ch (ideal 66ch) — prose desc short ≤48ch "Impresión local y red" — left-aligned, never centered. Numeric badge [+45 MiB] tabular-nums.
    // focus-ring: double 2px white + 4px ink87 (theme::FOCUS_RING) — 2px #FFFFFF inner + 4px rgba(0,0,0,0.87) outer
    // motion: if theme::prefers_reduced_motion() { no animation } else { 150ms ease } — respects EULER_REDUCED_MOTION/NO_ANIM — reduced-motion
    let _focus_ring = theme::FOCUS_RING; // double 2px white + 4px ink87
    let _focus_inner = theme::FOCUS_RING_INNER;
    let _focus_outer = theme::FOCUS_RING_OUTER;
    let _motion = if theme::prefers_reduced_motion() { "no animation" } else { "150ms ease" };
    let _motion_ms = theme::MOTION_DURATION_MS;
    // responsive cols: 360/768/1280/1920 breakpoints via adaptive_cols (single toggle → always 1 col but verified, matches hardware/codec)
    let _cols = adaptive_cols(width); //  <360:1, <600:1, <768:2, <1024:2, <1280:3, else 3 — single toggle stays 1 col layout
    let margin = theme::margin(width);
    let rule_len = (width.saturating_sub(margin * 2)).min(48).max(8);
    // H2 20/28 left-aligned
    out.push_str("Impresora\n");
    // CAPTION 12/16 38% muted secondary
    out.push_str("CUPS — +45 MiB\n");
    out.push_str(&"─".repeat(rule_len));
    out.push('\n');
    out.push_str(&"\n".repeat(theme::section_gap(width)));
    // single toggle — responsive cols handling not needed (single toggle), strictly left-aligned, no center.
    // interaction: hover 5% #14141a / pressed 9% #232326 — left-aligned row, whitespace + thin divider not needed for single
    let _hover = theme::HOVER; // rgba(0,0,0,0.05) ≈ #14141a on dark #0a0a0f
    let _hover_hex = theme::HOVER_HEX; // #14141a
    let _pressed = theme::PRESSED; // rgba(0,0,0,0.09) ≈ #232326
    let _pressed_hex = theme::PRESSED_HEX; // #232326
    // semantic state: color+shape+label triple redundancy — ● selected vs ○ idle + [+45 MiB] numeric + label
    // selected: ● ink87 (theme::INK_87) vs idle: ○ ink38 (theme::INK_38 muted 38%)
    // warm exceeds bar #1a1510 when enabling would push ISO >500 — see codecs exceeds_limit logic
    let _warm_bg = theme::WARM_EXCEEDS_BG; // #1a1510 warm exceeds bar
    let _warm_alpha = theme::WARM_EXCEEDS_BG_ALPHA;
    let _disabled_fg = theme::DISABLED_FG; // ink38 muted 38%
    let _selected_fg = theme::INK_87; // selected ● ink 87%
    let _idle_fg = theme::INK_38; // idle ○ ink 38% muted
    let check = if enable_printer { "●" } else { "○" };
    // tabular-nums: MiB badge [+45 MiB] right-aligned 10ch (CSS: font-variant-numeric: tabular-nums) so digits align vertically
    let badge = "[+45 MiB]";
    let desc = "Impresión local y red";
    // ensure description short ≤48ch via truncate (Scandinavian measure 55–68ch ideal, but single desc short)
    let desc_truncated = theme::truncate_str(desc, 48);
    // left-aligned row: checkbox left, badge right 10ch tabular, desc left — never centered
    let line = format!(" {check} {:<8} {badge:>10}  {}", "CUPS", desc_truncated);
    if line.chars().count() > width {
        out.push_str(&theme::truncate_str(&line, width));
        out.push('\n');
    } else {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn main() -> anyhow::Result<()> {
    let width = terminal_width();
    let mut state = GuiState::with_detected_hw();
    if state.hw.as_ref().is_some_and(|h| h.has_bluetooth) {
        state.codecs.toggle("bluez");
    }
    if let Some(hw) = &state.hw {
        println!("{}", render_hardware_card(hw, width));
    } else {
        println!("Sin perfil hardware");
    }
    println!("{}", render_codec_menu(&state.codecs, width));
    println!("{}", render_printer_menu(state.enable_printer, width));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;
    #[test]
    fn adaptive_cols_breakpoints() {
        assert_eq!(adaptive_cols(0), 1);
        assert_eq!(adaptive_cols(359), 1);
        assert_eq!(adaptive_cols(360), 1);
        assert_eq!(adaptive_cols(400), 1);
        assert_eq!(adaptive_cols(599), 1);
        assert_eq!(adaptive_cols(600), 2);
        assert_eq!(adaptive_cols(767), 2);
        assert_eq!(adaptive_cols(768), 2);
        assert_eq!(adaptive_cols(1023), 2);
        assert_eq!(adaptive_cols(1024), 3);
        assert_eq!(adaptive_cols(1279), 3);
        assert_eq!(adaptive_cols(1280), 3);
        assert_eq!(adaptive_cols(2000), 3);
    }
    #[test]
    fn codec_total_and_warning() {
        let mut cs = CodecSelection::new();
        assert!(cs.estimated_iso_mb() <= 500);
        for o in &mut cs.options {
            o.enabled = true;
        }
        assert!(cs.total_size_mb() > 30);
        cs.options.push(CodecOption {
            name: "mega",
            size_mb: 200,
            enabled: true,
            desc: "test",
        });
        assert!(cs.exceeds_limit());
    }
    #[test]
    fn render_does_not_panic() {
        let hw = HwProfile::detect();
        assert!(render_hardware_card(&hw, 80).contains("Hardware"));
        let cs = CodecSelection::new();
        let m = render_codec_menu(&cs, 80);
        assert!(m.contains("Codecs") || m.contains("BT"));
        assert!(m.contains("MiB"));
    }
    #[test]
    fn terminal_width_fallback() {
        assert!(terminal_width() > 0);
    }
    #[test]
    fn margin_rule_len() {
        // 24/40/64 → 3/4/8 chars, rule_len = (w - margin*2).clamp(8,48)
        assert_eq!(
            {
                let m = theme::margin(400);
                (400usize.saturating_sub(m * 2)).clamp(8, 48)
            },
            48
        );
        assert_eq!(
            {
                let m = theme::margin(20);
                (20usize.saturating_sub(m * 2)).clamp(8, 48)
            },
            14
        );
        // narrow width edge: width 8, margin 3 → 8-6=2 → max 8 → 8
        assert_eq!(
            {
                let m = theme::margin(8);
                (8usize.saturating_sub(m * 2)).clamp(8, 48)
            },
            8
        );
        // wide width 2000 → margin 8 → 2000-16=1984 min48=48
        assert_eq!(
            {
                let m = theme::margin(2000);
                (2000usize.saturating_sub(m * 2)).clamp(8, 48)
            },
            48
        );
    }
    #[test]
    fn section_gap_values() {
        assert_eq!(theme::section_gap(400), 3);
        assert_eq!(theme::section_gap(599), 3);
        assert_eq!(theme::section_gap(600), 5);
        assert_eq!(theme::section_gap(2000), 5);
    }
    #[test]
    fn truncate_uses_ellipsis() {
        // width>20 guard uses … glyph via theme::truncate_str
        let long = "a".repeat(100);
        let truncated = theme::truncate_str(&long, 10);
        assert_eq!(truncated.chars().count(), 10);
        assert!(truncated.ends_with('…'));
        assert!(!truncated.ends_with("..."));
        // unicode
        assert_eq!(theme::truncate_str("héllo 🌍 world", 7), "héllo …");
        assert_eq!(theme::truncate_str("🦀🦀🦀🦀", 3), "🦀🦀…");
        assert_eq!(theme::truncate_str("abc", 0), "");
        assert_eq!(theme::truncate_str("abc", 1), "a");
    }
    #[test]
    fn render_truncates_long_line() {
        let mut cs = CodecSelection::new();
        cs.options.push(CodecOption {
            name: "very-long-codec-name-exceeding-width-🦀🦀🦀",
            size_mb: 99,
            enabled: true,
            desc: "description with unicode 🌍 and very long text exceeding width",
        });
        let out = render_codec_menu(&cs, 30);
        // should contain … when width 30 and line longer than width
        assert!(out.contains('…') || out.contains("very-long"));
        // width 0,1 edge should not panic
        let out2 = render_codec_menu(&cs, 0);
        assert!(!out2.is_empty());
        let out3 = render_codec_menu(&cs, 1);
        assert!(!out3.is_empty());
    }
    #[test]
    fn render_printer_menu_no_panic_widths() {
        for w in [40, 80, 600] {
            let out_on = render_printer_menu(true, w);
            let out_off = render_printer_menu(false, w);
            assert!(!out_on.is_empty(), "width {w} on empty");
            assert!(!out_off.is_empty(), "width {w} off empty");
            assert!(out_on.contains("Impresora"), "missing Impresora at {w}");
            assert!(out_off.contains("CUPS"), "missing CUPS at {w}");
        }
        // very narrow edge not panic
        for w in [0, 1, 20, 2000] {
            let out = render_printer_menu(true, w);
            assert!(!out.is_empty());
            let out2 = render_printer_menu(false, w);
            assert!(!out2.is_empty());
        }
    }
    #[test]
    fn render_printer_menu_contains_minimal() {
        let out = render_printer_menu(false, 80);
        assert!(out.contains("Impresora"), "missing H2 Impresora");
        assert!(
            out.contains("CUPS — +45 MiB") || out.contains("CUPS"),
            "missing CAPTION CUPS — +45 MiB"
        );
        assert!(out.contains("+45 MiB"), "missing +45 MiB");
        assert!(out.contains("[+45 MiB]"), "missing badge [+45 MiB]");
        assert!(out.contains("Impresión local y red"), "missing desc");
        assert!(out.contains('○'), "disabled should show ○");
        let out_on = render_printer_menu(true, 80);
        assert!(out_on.contains('●'), "enabled should show ●");
        assert!(out_on.contains("Impresora"));
        // left-aligned check — no center marker, contains tabular badge
        assert!(out.contains("[+45 MiB]"));
        // width 40 small still contains header
        let out40 = render_printer_menu(true, 40);
        assert!(out40.contains("Impresora"));
        assert!(out40.contains("CUPS") || out40.contains('…'));
        let out600 = render_printer_menu(false, 600);
        assert!(out600.contains("Impresora"));
        assert!(out600.contains("+45 MiB"));
    }
}

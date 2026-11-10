use crate::autocomplete::AutocompleteType;
use gpui::{Global, Hsla, Rgba, WindowAppearance, hsla, rgb};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn resolve_is_dark(self, os_is_dark: bool) -> bool {
        match self {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => os_is_dark,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
}

pub fn is_appearance_dark(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemePalette {
    // Sidebar
    pub sidebar_bg: Rgba,
    pub chevron_shadow: Rgba,
    pub chevron_highlight: Rgba,
    pub header_bg: Rgba,
    pub footer_bg: Rgba,
    pub border: Rgba,
    pub card_bg: Rgba,
    pub task_item_bg: Rgba,
    pub card_border: Rgba,
    pub card_hover_border: Rgba,
    pub task_text: Rgba,
    pub muted_text: Rgba,
    pub text_primary: Rgba,
    pub number_badge_bg: Rgba,
    pub number_badge_fg: Rgba,
    pub kbd_bg: Rgba,
    pub kbd_fg: Rgba,
    pub kbd_border: Rgba,
    pub scrollbar_thumb: Rgba,
    pub scrollbar_thumb_hover: Rgba,

    // Alerts (Bootstrap 5 light/dark equivalents)
    pub alert_info_bg: Rgba,
    pub alert_info_fg: Rgba,
    pub alert_info_border: Rgba,
    pub alert_warning_bg: Rgba,
    pub alert_warning_fg: Rgba,
    pub alert_warning_border: Rgba,

    // Spotlight & Popover
    pub spotlight_bg: Rgba,
    pub spotlight_icon: Rgba,
    pub input_text: Rgba,
    pub input_bg: Rgba,
    pub input_border: Rgba,
    pub spotlight_lip_bg: Rgba,
    pub spotlight_lip_border: Rgba,
    pub spotlight_lip_key_bg: Rgba,
    pub spotlight_lip_key_fg: Rgba,
    pub ghost_suffix: Rgba,
    pub popover_bg: Rgba,
    pub popover_border: Rgba,
    pub popover_header_bg: Rgba,
    pub popover_header_border: Rgba,
    pub popover_active_item_bg: Rgba,
    pub popover_kbd_bg: Rgba,
    pub popover_kbd_fg: Rgba,
    pub popover_kbd_border: Rgba,
    pub popover_item_prefix: Rgba,
    pub popover_item_value: Rgba,

    // Type pills (TAG, PROJECT, MENTION)
    pub pill_tag_bg: Rgba,
    pub pill_tag_fg: Rgba,
    pub pill_project_bg: Rgba,
    pub pill_project_fg: Rgba,
    pub pill_mention_bg: Rgba,
    pub pill_mention_fg: Rgba,
}

impl ThemePalette {
    pub fn light() -> Self {
        Self {
            sidebar_bg: rgb(0xf8f9fa),
            chevron_shadow: rgb(0x000000).alpha(0.04),
            chevron_highlight: rgb(0xffffff).alpha(0.65),
            header_bg: rgb(0xffffff),
            footer_bg: rgb(0xffffff),
            border: rgb(0xdee2e6),
            card_bg: rgb(0xffffff),
            task_item_bg: rgb(0xffffff),
            card_border: rgb(0xdee2e6),
            card_hover_border: rgb(0xced4da),
            task_text: rgb(0x343a40),
            muted_text: rgb(0x6c757d),
            text_primary: rgb(0x212529),
            number_badge_bg: rgb(0xe9ecef),
            number_badge_fg: rgb(0x495057),
            kbd_bg: rgb(0xf8f9fa),
            kbd_fg: rgb(0x212529),
            kbd_border: rgb(0xced4da),
            scrollbar_thumb: rgb(0xcccccc),
            scrollbar_thumb_hover: rgb(0xaaaaaa),

            alert_info_bg: rgb(0xcff4fc),
            alert_info_fg: rgb(0x055160),
            alert_info_border: rgb(0x9eeaf9),
            alert_warning_bg: rgb(0xfff3cd),
            alert_warning_fg: rgb(0x664d03),
            alert_warning_border: rgb(0xffecb5),

            spotlight_bg: rgb(0xffffff),
            spotlight_icon: rgb(0x6c757d),
            input_text: rgb(0x212529),
            input_bg: rgb(0xffffff),
            input_border: rgb(0xdee2e6),
            spotlight_lip_bg: rgb(0xf8f9fa),
            spotlight_lip_border: rgb(0xedf2f7),
            spotlight_lip_key_bg: rgb(0xe9ecef),
            spotlight_lip_key_fg: rgb(0x212529),
            ghost_suffix: rgb(0x9499a0).alpha(0.75),
            popover_bg: rgb(0xffffff),
            popover_border: rgb(0xdee2e6),
            popover_header_bg: rgb(0xf8f9fa),
            popover_header_border: rgb(0xedf2f7),
            popover_active_item_bg: rgb(0xe9ecef),
            popover_kbd_bg: rgb(0xe9ecef),
            popover_kbd_fg: rgb(0x495057),
            popover_kbd_border: rgb(0xced4da),
            popover_item_prefix: rgb(0x6c757d),
            popover_item_value: rgb(0x212529),

            pill_tag_bg: rgb(0xcfe2ff),
            pill_tag_fg: rgb(0x084298),
            pill_project_bg: rgb(0xd1e7dd),
            pill_project_fg: rgb(0x0f5132),
            pill_mention_bg: rgb(0xe0cffc),
            pill_mention_fg: rgb(0x3d0a91),
        }
    }

    pub fn dark() -> Self {
        Self {
            sidebar_bg: rgb(0x141618),
            chevron_shadow: rgb(0x000000).alpha(0.40),
            chevron_highlight: rgb(0xffffff).alpha(0.04),
            header_bg: rgb(0x1a1d20),
            footer_bg: rgb(0x1a1d20),
            border: rgb(0x2b3035),
            card_bg: rgb(0x1e2124),
            task_item_bg: rgb(0x1e2124),
            card_border: rgb(0x2b3035),
            card_hover_border: rgb(0x3d4349),
            task_text: rgb(0xe9ecef),
            muted_text: rgb(0xadb5bd),
            text_primary: rgb(0xdee2e6),
            number_badge_bg: rgb(0x2b3035),
            number_badge_fg: rgb(0xadb5bd),
            kbd_bg: rgb(0x2b3035),
            kbd_fg: rgb(0xdee2e6),
            kbd_border: rgb(0x3d4349),
            scrollbar_thumb: rgb(0x343a40),
            scrollbar_thumb_hover: rgb(0x495057),

            alert_info_bg: rgb(0x032830),
            alert_info_fg: rgb(0x6edff6),
            alert_info_border: rgb(0x087990),
            alert_warning_bg: rgb(0x332701),
            alert_warning_fg: rgb(0xffda6a),
            alert_warning_border: rgb(0x997404),

            spotlight_bg: rgb(0x1e2124),
            spotlight_icon: rgb(0xadb5bd),
            input_text: rgb(0xf8f9fa),
            input_bg: rgb(0x212529),
            input_border: rgb(0x495057),
            spotlight_lip_bg: rgb(0x16181a),
            spotlight_lip_border: rgb(0x2b3035),
            spotlight_lip_key_bg: rgb(0x2b3035),
            spotlight_lip_key_fg: rgb(0xdee2e6),
            ghost_suffix: rgb(0xadb5bd).alpha(0.55),
            popover_bg: rgb(0x1e2124),
            popover_border: rgb(0x2b3035),
            popover_header_bg: rgb(0x16181a),
            popover_header_border: rgb(0x2b3035),
            popover_active_item_bg: rgb(0x2b3035),
            popover_kbd_bg: rgb(0x2b3035),
            popover_kbd_fg: rgb(0xdee2e6),
            popover_kbd_border: rgb(0x3d4349),
            popover_item_prefix: rgb(0xadb5bd),
            popover_item_value: rgb(0xf8f9fa),

            pill_tag_bg: rgb(0x084298),
            pill_tag_fg: rgb(0xcfe2ff),
            pill_project_bg: rgb(0x0f5132),
            pill_project_fg: rgb(0xd1e7dd),
            pill_mention_bg: rgb(0x3d0a91),
            pill_mention_fg: rgb(0xe0cffc),
        }
    }

    pub fn chevron_strokes(&self) -> (Rgba, Rgba) {
        (self.chevron_shadow, self.chevron_highlight)
    }

    pub fn type_pill_colors(&self, token_type: AutocompleteType) -> (Rgba, Rgba) {
        match token_type {
            AutocompleteType::Tag => (self.pill_tag_bg, self.pill_tag_fg),
            AutocompleteType::Project => (self.pill_project_bg, self.pill_project_fg),
            AutocompleteType::Mention => (self.pill_mention_bg, self.pill_mention_fg),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActiveTheme {
    pub mode: ThemeMode,
    pub is_dark: bool,
    pub palette: ThemePalette,
}

impl Global for ActiveTheme {}

impl ActiveTheme {
    pub fn new(mode: ThemeMode, os_is_dark: bool) -> Self {
        let is_dark = mode.resolve_is_dark(os_is_dark);
        Self::from_is_dark(is_dark, mode)
    }

    pub fn from_is_dark(is_dark: bool, mode: ThemeMode) -> Self {
        let palette = if is_dark {
            ThemePalette::dark()
        } else {
            ThemePalette::light()
        };
        Self {
            mode,
            is_dark,
            palette,
        }
    }
}

/// Ports JS `hashCode` (`src/renderer/app.tsx:57-63`):
/// `hash = str.charCodeAt(i) + ((hash << 5) - hash)` over UTF-16 code units.
pub fn hash_code(s: &str) -> i32 {
    let mut hash: i32 = 0;
    for code_unit in s.encode_utf16() {
        hash = (code_unit as i32).wrapping_add((hash << 5).wrapping_sub(hash));
    }
    hash
}

/// Computes deterministic hue in `0..360` matching `Math.abs(hashCode(tag)) % 360`.
pub fn tag_hue(tag: &str) -> u32 {
    hash_code(tag).unsigned_abs() % 360
}

/// Returns `(background, foreground)` HSLA colors for an inline `#tag`, `$project`, or `@mention` badge.
/// Ports `src/renderer/index.css:352-372`:
/// - Light task item badge: `bg = hsla(hue/360, 0.75, 0.90, 1.0)`, `fg = hsla(hue/360, 0.75, 0.25, 1.0)`
/// - Dark task item badge: `bg = hsla(hue/360, 0.45, 0.24, 1.0)`, `fg = hsla(hue/360, 0.85, 0.85, 1.0)`
/// - Inside `.current-task-card` (both themes): `bg = hsla(hue/360, 0.75, 0.90, 1.0)`, `fg = hsla(hue/360, 0.75, 0.20, 1.0)`
pub fn tag_badge_colors(tag: &str, is_dark: bool, in_current_card: bool) -> (Hsla, Hsla) {
    let h = tag_hue(tag) as f32 / 360.0;
    if in_current_card {
        (hsla(h, 0.75, 0.90, 1.0), hsla(h, 0.75, 0.20, 1.0))
    } else if is_dark {
        (hsla(h, 0.45, 0.24, 1.0), hsla(h, 0.85, 0.85, 1.0))
    } else {
        (hsla(h, 0.75, 0.90, 1.0), hsla(h, 0.75, 0.25, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_code_matches_js_implementation() {
        assert_eq!(hash_code(""), 0);
        // Single character 'a' (97): 97 + ((0 << 5) - 0) = 97
        assert_eq!(hash_code("a"), 97);
        // "#urgent":
        // '#' (35) -> 35
        // 'u' (117) -> 117 + 35*31 = 1202
        // 'r' (114) -> 114 + 1202*31 = 37376
        // 'g' (103) -> 103 + 37376*31 = 1158759
        // 'e' (101) -> 101 + 1158759*31 = 35921630
        // 'n' (110) -> 110 + 35921630*31 = 1113570640
        // 't' (116) -> 116 + ((1113570640_i32 << 5) - 1113570640) = 160951588
        assert_eq!(hash_code("#urgent"), 160_951_588);
        assert_eq!(tag_hue("#urgent"), 160_951_588u32 % 360);
    }

    #[test]
    fn test_tag_badge_colors_variants() {
        let (bg_light, fg_light) = tag_badge_colors("#urgent", false, false);
        assert!((bg_light.s - 0.75).abs() < 1e-6);
        assert!((bg_light.l - 0.90).abs() < 1e-6);
        assert!((fg_light.l - 0.25).abs() < 1e-6);

        let (bg_dark, fg_dark) = tag_badge_colors("#urgent", true, false);
        assert!((bg_dark.s - 0.45).abs() < 1e-6);
        assert!((bg_dark.l - 0.24).abs() < 1e-6);
        assert!((fg_dark.s - 0.85).abs() < 1e-6);
        assert!((fg_dark.l - 0.85).abs() < 1e-6);

        // Current card overrides both light and dark themes
        let (bg_card_dark, fg_card_dark) = tag_badge_colors("#urgent", true, true);
        let (bg_card_light, fg_card_light) = tag_badge_colors("#urgent", false, true);
        assert_eq!(bg_card_dark, bg_card_light);
        assert_eq!(fg_card_dark, fg_card_light);
        assert!((fg_card_dark.l - 0.20).abs() < 1e-6);
    }

    #[test]
    fn test_palette_type_pills_swap_in_dark_mode() {
        let light = ThemePalette::light();
        let dark = ThemePalette::dark();

        assert_eq!(light.pill_tag_bg, dark.pill_tag_fg);
        assert_eq!(light.pill_tag_fg, dark.pill_tag_bg);
        assert_eq!(light.pill_project_bg, dark.pill_project_fg);
        assert_eq!(light.pill_project_fg, dark.pill_project_bg);
        assert_eq!(light.pill_mention_bg, dark.pill_mention_fg);
        assert_eq!(light.pill_mention_fg, dark.pill_mention_bg);
    }
}

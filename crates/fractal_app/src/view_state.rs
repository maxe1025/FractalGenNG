/// Transient interaction state that never gets serialized or exported — kept
/// separate from `FractalParams`, which is the pure, serializable description
/// of a render.
pub struct ViewState {
    pub julia_seed_picker_active: bool,
    pub active_tab: PropertiesTab,
}

impl Default for ViewState {
    fn default() -> Self {
        Self { julia_seed_picker_active: false, active_tab: PropertiesTab::Fractal }
    }
}

/// Which properties-panel section is currently shown, Blender-Properties-
/// editor style (a narrow icon-tab column switches between them instead of
/// stacking everything into one long scroll).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertiesTab {
    Fractal,
    Palette,
    Bookmarks,
    Export,
    About,
}

impl PropertiesTab {
    pub const ALL: [PropertiesTab; 5] = [
        PropertiesTab::Fractal,
        PropertiesTab::Palette,
        PropertiesTab::Bookmarks,
        PropertiesTab::Export,
        PropertiesTab::About,
    ];

    /// Short glyph shown in the icon-tab column. Plain BMP/emoji glyphs from
    /// egui's bundled icon font — no custom font asset needed.
    pub fn icon(self) -> &'static str {
        match self {
            PropertiesTab::Fractal => "🌀",
            PropertiesTab::Palette => "🎨",
            PropertiesTab::Bookmarks => "⭐",
            PropertiesTab::Export => "💾",
            PropertiesTab::About => "ℹ",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PropertiesTab::Fractal => "Fractal",
            PropertiesTab::Palette => "Palette",
            PropertiesTab::Bookmarks => "Bookmarks",
            PropertiesTab::Export => "Export",
            PropertiesTab::About => "About",
        }
    }
}

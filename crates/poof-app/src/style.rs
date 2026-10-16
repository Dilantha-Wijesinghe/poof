//! Design tokens and widget styles.
//!
//! Views never hard-code colours: they call the functions in this module,
//! which derive everything from [`Tokens`] for the active light/dark theme.
//! Changing the look of the app means editing this file only.

use iced::border::Radius;
use iced::widget::{self, container, progress_bar, svg, text};
use iced::{Background, Border, Color, Shadow, Theme, Vector, color};

/// Spacing scale (px).
pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

/// Corner radii (px).
pub mod radius {
    pub const S: f32 = 6.0;
    pub const M: f32 = 10.0;
    pub const L: f32 = 14.0;
}

/// Type scale (px).
pub mod size {
    pub const CAPTION: f32 = 11.5;
    pub const BODY: f32 = 13.0;
    pub const LABEL: f32 = 14.0;
    pub const HEADING: f32 = 16.0;
    pub const TITLE: f32 = 24.0;
    pub const DISPLAY: f32 = 36.0;
}

#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub background: Color,
    pub sidebar: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub accent: Color,
    pub accent_strong: Color,
    pub accent_soft: Color,
    pub on_accent: Color,
    /// Brand companion to `accent`, used for purgeable space.
    pub violet: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub track: Color,
    pub shadow: Color,
    /// Categorical colours for treemaps and charts, tuned for both themes.
    pub series: [Color; 8],
}

/// Brand palette: Sky Blue, Cyan, Violet, Silver, White.
pub mod brand {
    use iced::{Color, color};
    pub const SKY: Color = color!(0x5b8cff);
    pub const CYAN: Color = color!(0x22d3ee);
    pub const VIOLET: Color = color!(0xa78bfa);
    pub const SILVER: Color = color!(0xe5eaf0);
    pub const WHITE: Color = color!(0xfafbff);
}

const SERIES: [Color; 8] = [
    color!(0x5b8cff), // sky
    color!(0xa78bfa), // violet
    color!(0x22d3ee), // cyan
    color!(0x818cf8), // indigo
    color!(0x38bdf8), // light blue
    color!(0xf472b6), // pink
    color!(0x34d399), // mint
    color!(0xfbbf24), // amber
];

pub const LIGHT: Tokens = Tokens {
    background: color!(0xf7f9fd),
    sidebar: color!(0xeef2f9),
    surface: brand::WHITE,
    surface_hover: color!(0xf1f4fb),
    border: color!(0xe2e8f2),
    text: color!(0x1b2233),
    muted: color!(0x5f6b82),
    faint: color!(0x9aa5b8),
    accent: brand::SKY,
    accent_strong: color!(0x3f6ff2),
    accent_soft: Color { a: 0.12, ..brand::SKY },
    on_accent: color!(0xffffff),
    violet: brand::VIOLET,
    success: color!(0x1fa874),
    warning: color!(0xe8930c),
    danger: color!(0xef4444),
    track: brand::SILVER,
    shadow: Color { a: 0.07, ..color!(0x1b3a8a) },
    series: SERIES,
};

pub const DARK: Tokens = Tokens {
    background: color!(0x0f1320),
    sidebar: color!(0x131929),
    surface: color!(0x182033),
    surface_hover: color!(0x1f2940),
    border: color!(0x26314a),
    text: color!(0xe8edf7),
    muted: color!(0x9aa6bf),
    faint: color!(0x5d6880),
    accent: color!(0x6e9bff),
    accent_strong: color!(0x8ab0ff),
    accent_soft: Color { a: 0.16, ..color!(0x6e9bff) },
    on_accent: color!(0xffffff),
    violet: brand::VIOLET,
    success: color!(0x34d399),
    warning: color!(0xfbbf24),
    danger: color!(0xf87171),
    track: color!(0x222c42),
    shadow: Color { a: 0.4, ..color!(0x000000) },
    series: SERIES,
};

/// The tokens for whichever theme is active.
pub fn tokens(theme: &Theme) -> &'static Tokens {
    if theme.extended_palette().is_dark { &DARK } else { &LIGHT }
}

/// Builds the iced [`Theme`] so built-in widgets (scrollbars, menus,
/// text inputs) pick up our palette too.
pub fn theme(dark: bool) -> Theme {
    let t = if dark { &DARK } else { &LIGHT };
    Theme::custom(
        if dark { "Poof Dark" } else { "Poof Light" },
        iced::theme::Palette {
            background: t.background,
            text: t.text,
            primary: t.accent,
            success: t.success,
            warning: t.warning,
            danger: t.danger,
        },
    )
}

pub fn alpha(color: Color, a: f32) -> Color {
    Color { a, ..color }
}

/// Mixes `a` towards `b` by `amount` (0 = a, 1 = b).
pub fn mix(a: Color, b: Color, amount: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * amount,
        g: a.g + (b.g - a.g) * amount,
        b: a.b + (b.b - a.b) * amount,
        a: a.a + (b.a - a.a) * amount,
    }
}

fn border(color: Color, radius: f32) -> Border {
    Border { color, width: 1.0, radius: Radius::from(radius) }
}

// ---------------------------------------------------------------- containers

pub fn page(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style { background: Some(t.background.into()), text_color: Some(t.text), ..Default::default() }
}

pub fn sidebar(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.sidebar.into()),
        border: Border { color: t.border, width: 0.0, radius: 0.0.into() },
        ..Default::default()
    }
}

pub fn card(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.surface.into()),
        border: border(t.border, radius::L),
        shadow: Shadow { color: t.shadow, offset: Vector::new(0.0, 1.0), blur_radius: 6.0 },
        ..Default::default()
    }
}

/// A subtle inset panel inside a card.
pub fn well(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.background.into()),
        border: border(t.border, radius::M),
        ..Default::default()
    }
}

/// A highlighted call-out, tinted with `tone`.
pub fn banner(tone: Tone) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let c = tone.color(theme);
        container::Style {
            background: Some(alpha(c, 0.10).into()),
            border: border(alpha(c, 0.30), radius::M),
            text_color: Some(tokens(theme).text),
            ..Default::default()
        }
    }
}

/// Small rounded label, e.g. "Safe" or "+2.4 GB".
pub fn badge(tone: Tone) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let c = tone.color(theme);
        container::Style {
            background: Some(alpha(c, 0.14).into()),
            text_color: Some(c),
            border: Border { radius: Radius::from(999.0), ..Border::default() },
            ..Default::default()
        }
    }
}

/// A solid swatch used for legends and bar segments.
pub fn swatch(color: Color, radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(color.into()),
        border: Border { radius: Radius::from(radius), ..Border::default() },
        ..Default::default()
    }
}

/// The brand gradient (cyan → sky → violet) at `angle` radians.
pub fn brand_gradient(angle: f32) -> Background {
    let gradient = iced::gradient::Linear::new(angle)
        .add_stop(0.0, brand::CYAN)
        .add_stop(0.5, brand::SKY)
        .add_stop(1.0, brand::VIOLET);
    Background::Gradient(gradient.into())
}

/// A filled bar segment painted with the brand gradient.
pub fn brand_fill(radius: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(brand_gradient(std::f32::consts::FRAC_PI_2)),
        border: Border { radius: Radius::from(radius), ..Border::default() },
        ..Default::default()
    }
}

pub fn tooltip(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.text.into()),
        text_color: Some(t.background),
        border: Border { radius: Radius::from(radius::S), ..Border::default() },
        ..Default::default()
    }
}

pub fn scrim(theme: &Theme) -> container::Style {
    let dark = theme.extended_palette().is_dark;
    container::Style {
        background: Some(alpha(Color::BLACK, if dark { 0.55 } else { 0.25 }).into()),
        ..Default::default()
    }
}

pub fn dialog(theme: &Theme) -> container::Style {
    let t = tokens(theme);
    container::Style {
        background: Some(t.surface.into()),
        border: border(t.border, radius::L),
        shadow: Shadow {
            color: alpha(Color::BLACK, 0.25),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 40.0,
        },
        text_color: Some(t.text),
        ..Default::default()
    }
}

pub fn toast(tone: Tone) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let t = tokens(theme);
        container::Style {
            background: Some(t.text.into()),
            text_color: Some(t.background),
            border: Border { color: tone.color(theme), width: 0.0, radius: Radius::from(radius::M) },
            shadow: Shadow {
                color: alpha(Color::BLACK, 0.2),
                offset: Vector::new(0.0, 6.0),
                blur_radius: 20.0,
            },
            ..Default::default()
        }
    }
}

// --------------------------------------------------------------------- tones

/// Semantic colour roles for badges, banners and icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Success,
    Warning,
    Danger,
    Neutral,
}

impl Tone {
    pub fn color(self, theme: &Theme) -> Color {
        let t = tokens(theme);
        match self {
            Tone::Accent => t.accent,
            Tone::Success => t.success,
            Tone::Warning => t.warning,
            Tone::Danger => t.danger,
            Tone::Neutral => t.muted,
        }
    }
}

// ---------------------------------------------------------------------- text

pub fn muted(theme: &Theme) -> text::Style {
    text::Style { color: Some(tokens(theme).muted) }
}

pub fn faint(theme: &Theme) -> text::Style {
    text::Style { color: Some(tokens(theme).faint) }
}

pub fn tone_text(tone: Tone) -> impl Fn(&Theme) -> text::Style {
    move |theme| text::Style { color: Some(tone.color(theme)) }
}

// ------------------------------------------------------------------- buttons

/// Visual weight of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Filled accent — the one main action on a screen.
    Primary,
    /// Outlined surface.
    Secondary,
    /// Text-only until hovered.
    Ghost,
    /// Filled red, for irreversible actions.
    Destructive,
}

pub fn button(kind: Kind) -> impl Fn(&Theme, widget::button::Status) -> widget::button::Style {
    move |theme, status| {
        let t = tokens(theme);
        let hovered = matches!(status, widget::button::Status::Hovered | widget::button::Status::Pressed);
        let disabled = status == widget::button::Status::Disabled;
        let (background, text_color, border_color) = match kind {
            Kind::Primary => {
                (Some(if hovered { t.accent_strong } else { t.accent }), t.on_accent, Color::TRANSPARENT)
            }
            Kind::Destructive => (
                Some(if hovered { mix(t.danger, Color::BLACK, 0.1) } else { t.danger }),
                t.on_accent,
                Color::TRANSPARENT,
            ),
            Kind::Secondary => (Some(if hovered { t.surface_hover } else { t.surface }), t.text, t.border),
            Kind::Ghost => (hovered.then_some(t.surface_hover), t.text, Color::TRANSPARENT),
        };
        let style = widget::button::Style {
            background: background.map(Background::Color),
            text_color,
            border: border(border_color, radius::M),
            shadow: Shadow::default(),
            snap: true,
        };
        if disabled {
            widget::button::Style {
                background: style.background.map(|b| b.scale_alpha(0.45)),
                text_color: alpha(style.text_color, 0.45),
                ..style
            }
        } else {
            style
        }
    }
}

/// A sidebar navigation item.
pub fn nav_item(active: bool) -> impl Fn(&Theme, widget::button::Status) -> widget::button::Style {
    move |theme, status| {
        let t = tokens(theme);
        let hovered = matches!(status, widget::button::Status::Hovered | widget::button::Status::Pressed);
        let background = if active {
            Some(t.surface.into())
        } else if hovered {
            Some(alpha(t.text, 0.05).into())
        } else {
            None
        };
        widget::button::Style {
            background,
            text_color: if active { t.text } else { t.muted },
            border: Border {
                color: if active { t.border } else { Color::TRANSPARENT },
                width: 1.0,
                radius: Radius::from(radius::M),
            },
            shadow: if active {
                Shadow { color: t.shadow, offset: Vector::new(0.0, 1.0), blur_radius: 3.0 }
            } else {
                Shadow::default()
            },
            snap: true,
        }
    }
}

/// A full-width list row that highlights on hover.
pub fn row_button(selected: bool) -> impl Fn(&Theme, widget::button::Status) -> widget::button::Style {
    move |theme, status| {
        let t = tokens(theme);
        let hovered = matches!(status, widget::button::Status::Hovered | widget::button::Status::Pressed);
        let background = if selected {
            Some(t.accent_soft.into())
        } else if hovered {
            Some(t.surface_hover.into())
        } else {
            None
        };
        widget::button::Style {
            background,
            text_color: t.text,
            border: Border { radius: Radius::from(radius::S), ..Border::default() },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// A large selectable tile (e.g. scan targets).
pub fn tile(theme: &Theme, status: widget::button::Status) -> widget::button::Style {
    let t = tokens(theme);
    let hovered = matches!(status, widget::button::Status::Hovered | widget::button::Status::Pressed);
    widget::button::Style {
        background: Some(t.surface.into()),
        text_color: t.text,
        border: border(if hovered { t.accent } else { t.border }, radius::L),
        shadow: Shadow {
            color: t.shadow,
            offset: Vector::new(0.0, if hovered { 4.0 } else { 1.0 }),
            blur_radius: if hovered { 16.0 } else { 6.0 },
        },
        snap: true,
    }
}

// --------------------------------------------------------------- form & misc

pub fn icon(color: fn(&Tokens) -> Color) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |theme, _| svg::Style { color: Some(color(tokens(theme))) }
}

pub fn icon_tone(tone: Tone) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |theme, _| svg::Style { color: Some(tone.color(theme)) }
}

pub fn checkbox(theme: &Theme, status: widget::checkbox::Status) -> widget::checkbox::Style {
    let t = tokens(theme);
    let (checked, hovered) = match status {
        widget::checkbox::Status::Hovered { is_checked } => (is_checked, true),
        widget::checkbox::Status::Active { is_checked }
        | widget::checkbox::Status::Disabled { is_checked } => (is_checked, false),
    };
    widget::checkbox::Style {
        background: if checked { t.accent.into() } else { t.surface.into() },
        icon_color: t.on_accent,
        border: border(
            if checked {
                t.accent
            } else if hovered {
                t.faint
            } else {
                t.border
            },
            radius::S - 1.0,
        ),
        text_color: Some(t.text),
    }
}

pub fn toggler(theme: &Theme, status: widget::toggler::Status) -> widget::toggler::Style {
    let t = tokens(theme);
    let on = match status {
        widget::toggler::Status::Active { is_toggled }
        | widget::toggler::Status::Hovered { is_toggled }
        | widget::toggler::Status::Disabled { is_toggled } => is_toggled,
    };
    let base = widget::toggler::default(theme, status);
    widget::toggler::Style {
        background: if on { t.accent.into() } else { t.track.into() },
        foreground: Color::WHITE.into(),
        background_border_color: Color::TRANSPARENT,
        foreground_border_color: Color::TRANSPARENT,
        ..base
    }
}

pub fn pick_list(theme: &Theme, status: widget::pick_list::Status) -> widget::pick_list::Style {
    let t = tokens(theme);
    let hovered =
        matches!(status, widget::pick_list::Status::Hovered | widget::pick_list::Status::Opened { .. });
    widget::pick_list::Style {
        text_color: t.text,
        placeholder_color: t.faint,
        handle_color: t.muted,
        background: if hovered { t.surface_hover.into() } else { t.surface.into() },
        border: border(t.border, radius::M),
    }
}

pub fn progress(theme: &Theme) -> progress_bar::Style {
    let t = tokens(theme);
    progress_bar::Style {
        background: t.track.into(),
        bar: t.accent.into(),
        border: Border { radius: Radius::from(999.0), ..Border::default() },
    }
}

/// A slim, unobtrusive scrollbar.
pub fn scrollable(theme: &Theme, status: widget::scrollable::Status) -> widget::scrollable::Style {
    let t = tokens(theme);
    let base = widget::scrollable::default(theme, status);
    let active = !matches!(status, widget::scrollable::Status::Active { .. });
    let rail = widget::scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: widget::scrollable::Scroller {
            background: alpha(t.text, if active { 0.28 } else { 0.14 }).into(),
            border: Border { radius: Radius::from(999.0), ..Border::default() },
        },
    };
    widget::scrollable::Style { vertical_rail: rail, horizontal_rail: rail, ..base }
}

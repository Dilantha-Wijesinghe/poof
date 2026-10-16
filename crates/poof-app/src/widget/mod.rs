//! Reusable view building blocks. Screens compose these instead of styling
//! raw widgets, which keeps the UI consistent and the screens short.

pub mod chart;
pub mod treemap;

use iced::widget::{
    Button, Container, Text, button, center, column, container, mouse_area, opaque, row, space, stack, text,
    tooltip,
};
use iced::{Alignment, Color, Element, Fill, FillPortion, Length, Padding};

use crate::font;
use crate::icon::Icon;
use crate::style::{self, Kind, Tone, size, space as gap};

// ---------------------------------------------------------------------- text

pub fn title<'a>(content: impl text::IntoFragment<'a>) -> Text<'a> {
    text(content).size(size::TITLE).font(font::SEMIBOLD)
}

pub fn heading<'a>(content: impl text::IntoFragment<'a>) -> Text<'a> {
    text(content).size(size::HEADING).font(font::SEMIBOLD)
}

pub fn label<'a>(content: impl text::IntoFragment<'a>) -> Text<'a> {
    text(content).size(size::LABEL).font(font::MEDIUM)
}

pub fn body<'a>(content: impl text::IntoFragment<'a>) -> Text<'a> {
    text(content).size(size::BODY)
}

pub fn muted<'a>(content: impl text::IntoFragment<'a>) -> Text<'a> {
    text(content).size(size::BODY).style(style::muted)
}

pub fn caption<'a>(content: impl text::IntoFragment<'a>) -> Text<'a> {
    text(content).size(size::CAPTION).style(style::muted)
}

/// Text kept on one line and clipped to the available width, for names
/// and paths that may be long.
pub fn single_line<'a, M: 'a>(content: Text<'a>) -> Element<'a, M> {
    container(content.wrapping(text::Wrapping::None)).width(Fill).clip(true).into()
}

/// Large numeric display, e.g. free space.
pub fn display<'a>(content: impl text::IntoFragment<'a>) -> Text<'a> {
    text(content).size(size::DISPLAY).font(font::BOLD)
}

// ---------------------------------------------------------------- containers

pub fn card<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Container<'a, M> {
    container(content).padding(20).width(Fill).style(style::card)
}

/// Title + subtitle on the left, actions on the right.
pub fn page_header<'a, M: 'a>(
    heading: &'a str,
    subtitle: impl text::IntoFragment<'a>,
    actions: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    row![column![title(heading), muted(subtitle)].spacing(gap::XS).width(Fill), actions.into()]
        .align_y(Alignment::Center)
        .spacing(gap::L)
        .into()
}

/// A small section label above a group of cards.
pub fn section<'a, M: 'a>(label_text: &'a str) -> Element<'a, M> {
    text(label_text.to_uppercase()).size(size::CAPTION).font(font::SEMIBOLD).style(style::faint).into()
}

pub fn badge<'a, M: 'a>(content: impl text::IntoFragment<'a>, tone: Tone) -> Element<'a, M> {
    container(text(content).size(size::CAPTION).font(font::SEMIBOLD))
        .padding([2, 8])
        .style(style::badge(tone))
        .into()
}

/// A tinted call-out with an icon, a message and an optional action.
pub fn banner<'a, M: Clone + 'a>(
    icon: Icon,
    tone: Tone,
    title_text: impl text::IntoFragment<'a>,
    message: impl text::IntoFragment<'a>,
    action: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let content = row![
        icon.view(18).style(style::icon_tone(tone)),
        column![label(title_text), muted(message)].spacing(2).width(Fill),
    ]
    .push(action)
    .spacing(gap::M)
    .align_y(Alignment::Center);
    container(content).padding([12, 16]).width(Fill).style(style::banner(tone)).into()
}

/// Centered icon, title, explanation and call to action.
pub fn empty_state<'a, M: Clone + 'a>(
    icon: Icon,
    heading_text: &'a str,
    message: impl text::IntoFragment<'a>,
    action: Option<Element<'a, M>>,
) -> Element<'a, M> {
    let mark = container(icon.view(26).style(style::icon(|t| t.accent)))
        .padding(14)
        .style(style::badge(Tone::Accent));
    let content = column![mark, heading(heading_text), muted(message).center()]
        .push(action)
        .spacing(gap::M)
        .align_x(Alignment::Center)
        .max_width(420);
    center(content).padding(gap::XXL).into()
}

// ------------------------------------------------------------------- buttons

/// A button with an optional leading icon.
pub fn btn<'a, M: Clone + 'a>(content: &'a str, icon: Option<Icon>, kind: Kind) -> Button<'a, M> {
    let tint = match kind {
        Kind::Primary | Kind::Destructive => style::icon(|t| t.on_accent),
        Kind::Secondary | Kind::Ghost => style::icon(|t| t.text),
    };
    let inner = row![]
        .push(icon.map(|i| i.view(15).style(tint)))
        .push(text(content).size(size::BODY).font(font::MEDIUM))
        .spacing(gap::S)
        .align_y(Alignment::Center);
    button(inner).padding([7, 14]).style(style::button(kind))
}

/// A square, icon-only ghost button with a hover tooltip.
pub fn icon_button<'a, M: Clone + 'a>(icon: Icon, hint: &'a str, on_press: Option<M>) -> Element<'a, M> {
    let b = button(icon.view(15)).padding(6).style(style::button(Kind::Ghost)).on_press_maybe(on_press);
    tooltip(
        b,
        container(caption(hint).style(|_| text::Style { color: None })).padding([4, 8]).style(style::tooltip),
        tooltip::Position::Top,
    )
    .gap(4)
    .into()
}

// ---------------------------------------------------------------------- bars

/// How a bar segment is filled.
#[derive(Debug, Clone, Copy)]
pub enum Paint {
    /// A colour resolved from the active theme's tokens.
    Token(fn(&style::Tokens) -> Color),
    /// The cyan → sky → violet brand gradient.
    Brand,
}

impl Paint {
    fn style(self, radius: f32) -> impl Fn(&iced::Theme) -> container::Style {
        move |theme| match self {
            Paint::Token(pick) => style::swatch(pick(style::tokens(theme)), radius)(theme),
            Paint::Brand => style::brand_fill(radius)(theme),
        }
    }
}

/// A horizontal bar made of segments; `segments` are `(fraction, paint)`
/// pairs and the remainder shows the track.
pub fn segmented_bar<'a, M: 'a>(segments: &[(f32, Paint)], height: f32) -> Element<'a, M> {
    const SCALE: f32 = 1000.0;
    let round = height / 2.0;
    let mut used = 0u16;
    let mut bar = row![].spacing(2).height(height).width(Fill);
    for &(fraction, paint) in segments {
        let portion = (fraction.clamp(0.0, 1.0) * SCALE).round() as u16;
        if portion == 0 {
            continue;
        }
        used = used.saturating_add(portion);
        let segment = container(space()).width(FillPortion(portion)).height(Fill);
        bar = bar.push(segment.style(paint.style(round)));
    }
    let rest = (SCALE as u16).saturating_sub(used);
    if rest > 0 {
        bar = bar.push(
            container(space())
                .width(FillPortion(rest))
                .height(Fill)
                .style(Paint::Token(|t| t.track).style(round)),
        );
    }
    bar.into()
}

/// A single-value meter painted with the brand gradient.
pub fn meter<'a, M: 'a>(fraction: f32, height: f32) -> Element<'a, M> {
    segmented_bar(&[(fraction, Paint::Brand)], height)
}

/// A 1px horizontal hairline.
pub fn divider<'a, M: 'a>() -> Element<'a, M> {
    container(space()).width(Fill).height(1).style(Paint::Token(|t| t.border).style(0.0)).into()
}

/// A small coloured dot followed by a label and value — for legends.
pub fn legend<'a, M: 'a>(paint: Paint, name: &'a str, value: String) -> Element<'a, M> {
    let dot = container(space()).width(8).height(8).style(paint.style(4.0));
    row![dot, caption(name), text(value).size(size::CAPTION).font(font::SEMIBOLD),]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}

// -------------------------------------------------------------------- dialog

/// A confirmation dialog description, rendered by [`modal`].
#[derive(Debug, Clone)]
pub struct Dialog<M> {
    pub title: String,
    pub message: String,
    pub details: Vec<String>,
    pub confirm: String,
    pub destructive: bool,
    pub on_confirm: M,
    pub on_cancel: M,
}

impl<M> Dialog<M> {
    pub fn map<N>(self, f: impl Fn(M) -> N) -> Dialog<N> {
        Dialog {
            title: self.title,
            message: self.message,
            details: self.details,
            confirm: self.confirm,
            destructive: self.destructive,
            on_confirm: f(self.on_confirm),
            on_cancel: f(self.on_cancel),
        }
    }
}

/// Overlays `dialog` (if any) on top of `base`, dimming the background.
///
/// The overlay layer is always present, even when empty, so opening a
/// dialog doesn't rebuild `base` and lose its scroll position.
pub fn modal<'a, M: Clone + 'a>(base: Element<'a, M>, dialog: Option<Dialog<M>>) -> Element<'a, M> {
    let Some(dialog) = dialog else { return stack![base, space()].into() };
    let details = dialog
        .details
        .into_iter()
        .fold(column![].spacing(gap::XS), |col, d| col.push(row![caption("•"), caption(d)].spacing(gap::S)));
    let kind = if dialog.destructive { Kind::Destructive } else { Kind::Primary };
    let confirm = button(text(dialog.confirm).size(size::BODY).font(font::MEDIUM))
        .padding([7, 14])
        .style(style::button(kind))
        .on_press(dialog.on_confirm);
    let card = container(
        column![
            heading(dialog.title),
            muted(dialog.message),
            details,
            row![
                space::horizontal(),
                btn("Cancel", None, Kind::Secondary).on_press(dialog.on_cancel.clone()),
                confirm
            ]
            .spacing(gap::S),
        ]
        .spacing(gap::M),
    )
    .padding(gap::XL)
    .max_width(480)
    .style(style::dialog);

    let overlay = mouse_area(center(opaque(card)).style(style::scrim)).on_press(dialog.on_cancel);
    stack![base, opaque(overlay)].into()
}

/// A content column with consistent page padding, scrolling vertically.
pub fn page_scroll<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    iced::widget::scrollable(container(content).padding(Padding {
        top: 8.0,
        right: 32.0,
        bottom: 32.0,
        left: 32.0,
    }))
    .style(style::scrollable)
    .height(Length::Fill)
    .into()
}

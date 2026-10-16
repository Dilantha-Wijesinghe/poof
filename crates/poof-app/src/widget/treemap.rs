//! An interactive, two-level squarified treemap of a folder.
//!
//! Each child of the current folder gets a tile sized by its disk usage;
//! large folder tiles also show their own children inside. Hovering reports
//! the item under the cursor and clicking asks to open it.

use iced::mouse::{self, Cursor};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, alignment};
use poof_core::scan::{NodeId, NodeKind, Tree};
use poof_core::size::ByteSize;
use poof_core::treemap::{Rect, squarify};

use crate::font;
use crate::style::{self, mix, tokens};

/// What the treemap reports to its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Hover(Option<NodeId>),
    Open(NodeId),
}

pub struct Treemap<'a> {
    pub tree: &'a Tree,
    pub node: NodeId,
    pub selected: Option<NodeId>,
}

impl std::fmt::Debug for Treemap<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Treemap").field("node", &self.node).finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub struct State {
    hovered: Option<NodeId>,
}

/// Tiles drawn for one top-level child.
const MAX_CHILDREN: usize = 250;
const MAX_NESTED: usize = 60;
const GAP: f32 = 2.0;
const HEADER: f32 = 20.0;

#[derive(Debug, Clone, Copy)]
struct Tile {
    id: NodeId,
    rect: Rect,
    /// 0 for children of the current folder, 1 for grandchildren.
    depth: u8,
    /// Index of the top-level tile this belongs to (drives the colour).
    group: usize,
    /// A folder tile that shows its children inside.
    nested: bool,
}

impl Treemap<'_> {
    fn layout(&self, bounds: Size) -> Vec<Tile> {
        let children: Vec<NodeId> = self
            .tree
            .children(self.node)
            .filter(|(_, n)| n.size > 0)
            .take(MAX_CHILDREN)
            .map(|(id, _)| id)
            .collect();
        let weights: Vec<u64> = children.iter().map(|&id| self.tree.get(id).size).collect();
        let rects = squarify(&weights, Rect::new(0.0, 0.0, bounds.width, bounds.height));

        let mut tiles = Vec::with_capacity(children.len() * 4);
        for (group, (&id, rect)) in children.iter().zip(rects).enumerate() {
            let rect = rect.inset(GAP / 2.0);
            let node = self.tree.get(id);
            let nested =
                node.is_dir() && !node.children.is_empty() && rect.width > 96.0 && rect.height > 64.0;
            tiles.push(Tile { id, rect, depth: 0, group, nested });
            if nested {
                let inner =
                    Rect::new(rect.x + 4.0, rect.y + HEADER, rect.width - 8.0, rect.height - HEADER - 4.0);
                let grand: Vec<(NodeId, u64)> = self
                    .tree
                    .children(id)
                    .filter(|(_, n)| n.size > 0)
                    .take(MAX_NESTED)
                    .map(|(c, n)| (c, n.size))
                    .collect();
                let weights: Vec<u64> = grand.iter().map(|g| g.1).collect();
                let rects = squarify(&weights, inner);
                for ((c, _), r) in grand.iter().zip(rects) {
                    tiles.push(Tile { id: *c, rect: r.inset(GAP / 2.0), depth: 1, group, nested: false });
                }
            }
        }
        tiles
    }

    fn hit(tiles: &[Tile], point: Point) -> Option<&Tile> {
        // Nested tiles come after their parent, so search back-to-front.
        tiles.iter().rev().find(|t| t.rect.contains(point.x, point.y) && t.rect.area() > 0.0)
    }

    /// What clicking a tile opens: folders open directly; files open their
    /// containing folder (for nested tiles) or nothing.
    fn target(&self, tile: &Tile) -> Option<NodeId> {
        let node = self.tree.get(tile.id);
        if node.is_dir() && !node.children.is_empty() {
            Some(tile.id)
        } else if tile.depth == 1 {
            node.parent
        } else {
            None
        }
    }
}

impl canvas::Program<Event> for Treemap<'_> {
    type State = State;

    fn update(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<canvas::Action<Event>> {
        let iced::Event::Mouse(event) = event else { return None };
        let position = cursor.position_in(bounds);
        match event {
            mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft => {
                let hovered = position.and_then(|p| Self::hit(&self.layout(bounds.size()), p).map(|t| t.id));
                (hovered != state.hovered).then(|| {
                    state.hovered = hovered;
                    canvas::Action::publish(Event::Hover(hovered))
                })
            }
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let tiles = self.layout(bounds.size());
                let target = position.and_then(|p| Self::hit(&tiles, p)).and_then(|t| self.target(t))?;
                Some(canvas::Action::publish(Event::Open(target)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let t = tokens(theme);
        let dark = theme.extended_palette().is_dark;
        let mut frame = Frame::new(renderer, bounds.size());

        for tile in self.layout(bounds.size()) {
            let node = self.tree.get(tile.id);
            let r = tile.rect;
            if r.width < 1.0 || r.height < 1.0 {
                continue;
            }
            let base =
                if node.kind == NodeKind::Grouped { t.faint } else { t.series[tile.group % t.series.len()] };
            let fill = match (tile.depth, tile.nested) {
                // Container tiles are a soft wash of their colour.
                (0, true) => mix(base, t.surface, if dark { 0.72 } else { 0.78 }),
                (0, false) => mix(base, t.surface, 0.12),
                // Vary nested tiles slightly so neighbours stay distinguishable.
                _ => mix(base, t.surface, 0.05 + 0.08 * (tile.id.index() % 4) as f32),
            };
            let radius = if tile.depth == 0 { 6.0 } else { 3.0 };
            let path =
                Path::rounded_rectangle(Point::new(r.x, r.y), Size::new(r.width, r.height), radius.into());
            frame.fill(&path, fill);

            let hovered = state.hovered == Some(tile.id);
            if hovered {
                frame.fill(&path, style::alpha(Color::WHITE, 0.18));
            }
            if hovered || self.selected == Some(tile.id) {
                let color = if hovered { t.text } else { t.accent };
                frame.stroke(&path, Stroke::default().with_color(color).with_width(2.0));
            }

            let ink = if tile.nested { t.text } else { readable_on(fill) };
            draw_label(&mut frame, node.name.as_ref(), node.size, r, tile, ink);
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, state: &State, _bounds: Rectangle, _cursor: Cursor) -> mouse::Interaction {
        if state.hovered.is_some() { mouse::Interaction::Pointer } else { mouse::Interaction::default() }
    }
}

/// Draws the name (and size, if there's room) clipped to the tile.
fn draw_label(frame: &mut Frame, name: &str, size: u64, r: Rect, tile: Tile, ink: Color) {
    let (min_w, min_h) = if tile.nested { (60.0, 18.0) } else { (48.0, 30.0) };
    if r.width < min_w || r.height < min_h {
        return;
    }
    let clip = Rectangle { x: r.x, y: r.y, width: r.width, height: r.height };
    let text_size = if tile.depth == 0 { 12.0 } else { 11.0 };
    frame.with_clip(clip, |f| {
        let text = |content: String, y: f32, font, alpha: f32| Text {
            content,
            position: Point::new(r.x + 8.0, r.y + y),
            color: style::alpha(ink, alpha),
            size: text_size.into(),
            font,
            align_x: iced::widget::text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            max_width: f32::INFINITY,
            ..Text::default()
        };
        if tile.nested {
            f.fill_text(text(format!("{name}  ·  {}", ByteSize(size)), 4.0, font::SEMIBOLD, 0.9));
        } else {
            f.fill_text(text(name.to_owned(), 7.0, font::SEMIBOLD, 0.95));
            if r.height > 44.0 {
                f.fill_text(text(ByteSize(size).to_string(), 23.0, font::REGULAR, 0.8));
            }
        }
    });
}

/// Black or white, whichever reads better on `background`.
fn readable_on(background: Color) -> Color {
    let luminance = 0.2126 * background.r + 0.7152 * background.g + 0.0722 * background.b;
    if luminance > 0.62 { Color::from_rgb(0.1, 0.1, 0.1) } else { Color::WHITE }
}

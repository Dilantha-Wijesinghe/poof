//! A line chart of available space over time, with hover read-out. Also
//! used as a compact sparkline.

use iced::mouse::{self, Cursor};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text, gradient};
use iced::{Point, Rectangle, Renderer, Theme, alignment};
use poof_core::monitor::Sample;
use poof_core::size::ByteSize;
use poof_core::time;

use crate::font;
use crate::style::{alpha, tokens};

#[derive(Debug)]
pub struct Chart<'a> {
    pub samples: &'a [Sample],
    /// Sparkline mode: no axes, labels or hover.
    pub compact: bool,
}

#[derive(Debug, Default)]
pub struct State {
    cursor: Option<Point>,
}

const LABEL_BAND: f32 = 22.0;

impl Chart<'_> {
    /// Maps samples to points inside `area`.
    fn points(&self, area: Rectangle) -> Vec<Point> {
        let (Some(first), Some(last)) = (self.samples.first(), self.samples.last()) else {
            return Vec::new();
        };
        let span = (last.at - first.at).max(1) as f32;
        let (lo, hi) = self.range();
        let height = (hi - lo).max(1.0);
        self.samples
            .iter()
            .map(|s| {
                let x = area.x + area.width * (s.at - first.at) as f32 / span;
                let y = area.y + area.height * (1.0 - (s.available as f32 - lo) / height);
                Point::new(x, y)
            })
            .collect()
    }

    /// Y range with headroom, so a flat line doesn't hug an edge.
    fn range(&self) -> (f32, f32) {
        let min = self.samples.iter().map(|s| s.available).min().unwrap_or(0) as f32;
        let max = self.samples.iter().map(|s| s.available).max().unwrap_or(0) as f32;
        let pad = ((max - min) * 0.15).max(1e9);
        ((min - pad).max(0.0), max + pad)
    }

    fn plot_area(&self, bounds: Rectangle) -> Rectangle {
        let bottom = if self.compact { 2.0 } else { LABEL_BAND };
        Rectangle { x: 2.0, y: 4.0, width: bounds.width - 4.0, height: bounds.height - 4.0 - bottom }
    }
}

impl canvas::Program<()> for Chart<'_> {
    type State = State;

    fn update(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<canvas::Action<()>> {
        if self.compact {
            return None;
        }
        match event {
            iced::Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let position = cursor.position_in(bounds);
                (position != state.cursor).then(|| {
                    state.cursor = position;
                    canvas::Action::request_redraw()
                })
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
        let mut frame = Frame::new(renderer, bounds.size());
        let area = self.plot_area(bounds);
        let points = self.points(area);
        if points.len() < 2 {
            return vec![frame.into_geometry()];
        }

        // Area under the line, fading out downwards.
        let fill = Path::new(|p| {
            p.move_to(Point::new(points[0].x, area.y + area.height));
            for &pt in &points {
                p.line_to(pt);
            }
            p.line_to(Point::new(points[points.len() - 1].x, area.y + area.height));
            p.close();
        });
        let wash = gradient::Linear::new(Point::new(0.0, area.y), Point::new(0.0, area.y + area.height))
            .add_stop(0.0, alpha(t.accent, 0.28))
            .add_stop(1.0, alpha(t.accent, 0.0));
        frame.fill(&fill, wash);

        let line = Path::new(|p| {
            p.move_to(points[0]);
            for &pt in &points[1..] {
                p.line_to(pt);
            }
        });
        frame.stroke(
            &line,
            Stroke::default().with_color(t.accent).with_width(if self.compact { 1.6 } else { 2.0 }),
        );

        if self.compact {
            return vec![frame.into_geometry()];
        }

        let label = |content: String, position: Point, align_x| Text {
            content,
            position,
            color: t.faint,
            size: 11.0.into(),
            font: font::REGULAR,
            align_x,
            align_y: alignment::Vertical::Bottom,
            ..Text::default()
        };
        let first = self.samples[0].at;
        let last = self.samples[self.samples.len() - 1].at;
        frame.fill_text(label(
            time::date(first),
            Point::new(area.x, bounds.height),
            iced::widget::text::Alignment::Left,
        ));
        frame.fill_text(label(
            time::date(last),
            Point::new(area.x + area.width, bounds.height),
            iced::widget::text::Alignment::Right,
        ));

        // Hover read-out: nearest sample to the cursor.
        if let Some(cursor) = state.cursor
            && let Some((i, pt)) = points
                .iter()
                .enumerate()
                .min_by(|a, b| (a.1.x - cursor.x).abs().total_cmp(&(b.1.x - cursor.x).abs()))
        {
            let guide = Path::line(Point::new(pt.x, area.y), Point::new(pt.x, area.y + area.height));
            frame.stroke(&guide, Stroke::default().with_color(alpha(t.text, 0.18)).with_width(1.0));
            frame.fill(&Path::circle(*pt, 4.5), t.surface);
            frame.stroke(&Path::circle(*pt, 4.5), Stroke::default().with_color(t.accent).with_width(2.0));

            let sample = self.samples[i];
            let right_half = pt.x > area.x + area.width / 2.0;
            let text = Text {
                content: format!("{} · {}", ByteSize(sample.available), time::date_time(sample.at)),
                position: Point::new(
                    if right_half { pt.x - 10.0 } else { pt.x + 10.0 },
                    (pt.y - 10.0).max(14.0),
                ),
                color: t.text,
                size: 12.0.into(),
                font: font::SEMIBOLD,
                align_x: if right_half {
                    iced::widget::text::Alignment::Right
                } else {
                    iced::widget::text::Alignment::Left
                },
                align_y: alignment::Vertical::Bottom,
                ..Text::default()
            };
            frame.fill_text(text);
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, _: &State, _: Rectangle, _: Cursor) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}

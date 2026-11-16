//! Hand-drawn pieces: the timer ring, bar charts, week grids and chains.

use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, LineCap, Path, Stroke, Text, path::Arc};
use iced::{Color, Pixels, Point, Radians, Rectangle, Renderer, Size, Theme, alignment};
use std::f32::consts::{FRAC_PI_2, TAU};

/// A circular progress ring.
pub struct Ring {
    pub progress: f32,
    pub color: Color,
    pub track: Color,
    pub thickness: f32,
}

impl<M> canvas::Program<M> for Ring {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = frame.center();
        let radius = bounds.width.min(bounds.height) / 2.0 - self.thickness / 2.0 - 1.0;
        frame.stroke(
            &Path::circle(center, radius),
            Stroke::default()
                .with_color(self.track)
                .with_width(self.thickness),
        );
        let p = self.progress.clamp(0.0, 1.0);
        if p > 0.001 {
            let arc = Path::new(|b| {
                b.arc(Arc {
                    center,
                    radius,
                    start_angle: Radians(-FRAC_PI_2),
                    end_angle: Radians(-FRAC_PI_2 + TAU * p.min(0.9999)),
                })
            });
            frame.stroke(
                &arc,
                Stroke::default()
                    .with_color(self.color)
                    .with_width(self.thickness)
                    .with_line_cap(LineCap::Round),
            );
        }
        vec![frame.into_geometry()]
    }
}

/// Vertical bars with optional labels under some of them.
pub struct Bars {
    pub values: Vec<f32>,
    /// Same length as `values`; empty strings are not drawn.
    pub labels: Vec<String>,
    pub color: Color,
    pub text: Color,
    pub grid: Color,
    /// Index drawn at full strength; the rest are slightly muted.
    pub highlight: Option<usize>,
    /// A horizontal reference line, in the same unit as `values`.
    pub goal: Option<f32>,
}

impl<M> canvas::Program<M> for Bars {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let n = self.values.len();
        if n == 0 {
            return vec![frame.into_geometry()];
        }
        let label_h = if self.labels.iter().any(|l| !l.is_empty()) {
            18.0
        } else {
            0.0
        };
        let h = (bounds.height - label_h - 4.0).max(1.0);
        let max = self
            .values
            .iter()
            .copied()
            .fold(self.goal.unwrap_or(0.0), f32::max)
            .max(1e-6);
        let slot = bounds.width / n as f32;
        let bar_w = (slot * 0.68).clamp(1.0, 28.0);

        frame.fill_rectangle(
            Point::new(0.0, h + 2.0),
            Size::new(bounds.width, 1.0),
            self.grid,
        );
        if let Some(goal) = self.goal {
            let y = 2.0 + h - h * (goal / max);
            let mut x = 0.0;
            while x < bounds.width {
                frame.fill_rectangle(Point::new(x, y), Size::new(4.0, 1.0), self.grid);
                x += 8.0;
            }
        }
        for (i, v) in self.values.iter().enumerate() {
            let x = slot * i as f32 + (slot - bar_w) / 2.0;
            let bh = (h * (v / max)).max(if *v > 0.0 { 2.0 } else { 0.0 });
            if bh > 0.0 {
                let strong = self.highlight.is_none_or(|hi| hi == i);
                let color = Color {
                    a: self.color.a * if strong { 1.0 } else { 0.55 },
                    ..self.color
                };
                let r = (bar_w / 2.0).min(3.0).min(bh / 2.0);
                frame.fill(
                    &Path::rounded_rectangle(
                        Point::new(x, 2.0 + h - bh),
                        Size::new(bar_w, bh),
                        r.into(),
                    ),
                    color,
                );
            }
            if let Some(label) = self.labels.get(i).filter(|l| !l.is_empty()) {
                frame.fill_text(Text {
                    content: label.clone(),
                    position: Point::new(slot * i as f32 + slot / 2.0, h + 6.0),
                    color: self.text,
                    size: Pixels(10.0),
                    align_x: iced::widget::text::Alignment::Center,
                    align_y: alignment::Vertical::Top,
                    ..Text::default()
                });
            }
        }
        vec![frame.into_geometry()]
    }
}

/// How a cell in a week grid or chain is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cell {
    /// Not part of the range: left blank.
    None,
    /// Faint placeholder.
    Empty,
    /// Filled with a colour.
    Fill(Color),
    /// Outlined only.
    Ring(Color),
    /// A small dot in the middle.
    Dot(Color),
}

/// Weeks as columns, Monday at the top: a contribution-style grid.
pub struct WeekGrid {
    /// In date order, starting on a Monday.
    pub cells: Vec<Cell>,
    pub empty: Color,
    pub cell: f32,
    pub gap: f32,
}

impl WeekGrid {
    pub fn size(&self) -> Size {
        let weeks = self.cells.len().div_ceil(7) as f32;
        Size::new(
            weeks * (self.cell + self.gap) - self.gap,
            7.0 * (self.cell + self.gap) - self.gap,
        )
    }
}

fn draw_cell(frame: &mut Frame, cell: Cell, at: Point, size: f32, empty: Color) {
    let r = (size * 0.25).min(4.0);
    let rect = |inset: f32| {
        Path::rounded_rectangle(
            Point::new(at.x + inset, at.y + inset),
            Size::new(size - 2.0 * inset, size - 2.0 * inset),
            r.into(),
        )
    };
    match cell {
        Cell::None => {}
        Cell::Empty => frame.fill(&rect(0.0), empty),
        Cell::Fill(c) => frame.fill(&rect(0.0), c),
        Cell::Ring(c) => {
            frame.fill(&rect(0.0), empty);
            frame.stroke(&rect(0.75), Stroke::default().with_color(c).with_width(1.5));
        }
        Cell::Dot(c) => {
            frame.fill(&rect(0.0), empty);
            frame.fill(
                &Path::circle(
                    Point::new(at.x + size / 2.0, at.y + size / 2.0),
                    size * 0.18,
                ),
                c,
            );
        }
    }
}

impl<M> canvas::Program<M> for WeekGrid {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let step = self.cell + self.gap;
        for (i, cell) in self.cells.iter().enumerate() {
            let at = Point::new((i / 7) as f32 * step, (i % 7) as f32 * step);
            draw_cell(&mut frame, *cell, at, self.cell, self.empty);
        }
        vec![frame.into_geometry()]
    }
}

/// A row of links. Neighbouring links that are both `joined` are connected,
/// which is what makes a streak read as a chain.
pub struct Chain {
    pub cells: Vec<Cell>,
    /// Whether each link is part of an unbroken run.
    pub joined: Vec<bool>,
    pub color: Color,
    pub empty: Color,
    pub cell: f32,
}

impl Chain {
    pub fn size(&self) -> Size {
        Size::new(
            self.cells.len() as f32 * self.cell * 1.5 - self.cell * 0.5,
            self.cell,
        )
    }
}

impl<M> canvas::Program<M> for Chain {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let step = self.cell * 1.5;
        let mid = self.cell / 2.0;
        for i in 1..self.cells.len() {
            if self.joined.get(i - 1) == Some(&true) && self.joined.get(i) == Some(&true) {
                frame.fill_rectangle(
                    Point::new((i - 1) as f32 * step + mid, mid - 2.0),
                    Size::new(step, 4.0),
                    self.color,
                );
            }
        }
        for (i, cell) in self.cells.iter().enumerate() {
            let c = Point::new(i as f32 * step + mid, mid);
            match *cell {
                Cell::None => {}
                Cell::Empty => frame.fill(&Path::circle(c, mid * 0.45), self.empty),
                Cell::Fill(col) => frame.fill(&Path::circle(c, mid), col),
                Cell::Ring(col) => frame.stroke(
                    &Path::circle(c, mid - 1.0),
                    Stroke::default().with_color(col).with_width(2.0),
                ),
                Cell::Dot(col) => frame.fill(&Path::circle(c, mid * 0.45), col),
            }
        }
        vec![frame.into_geometry()]
    }
}

//! Squarified treemap layout (Bruls, Huizing & van Wijk, 2000).
//!
//! Pure geometry: give it weights sorted largest-first and a rectangle, get
//! back one rectangle per weight whose area is proportional to it and whose
//! aspect ratio is as close to square as the algorithm allows.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Shrinks the rectangle by `amount` on every side (never below zero).
    #[must_use]
    pub fn inset(&self, amount: f32) -> Self {
        let width = (self.width - 2.0 * amount).max(0.0);
        let height = (self.height - 2.0 * amount).max(0.0);
        Self::new(self.x + amount, self.y + amount, width, height)
    }
}

/// Lays out `weights` (largest first) inside `bounds`. Zero weights get empty
/// rectangles so the output always lines up index-for-index with the input.
pub fn squarify(weights: &[u64], bounds: Rect) -> Vec<Rect> {
    let mut out = vec![Rect::default(); weights.len()];
    let total: f64 = weights.iter().map(|&w| w as f64).sum();
    if total <= 0.0 || bounds.area() <= 0.0 {
        return out;
    }

    // Scale weights to areas in the target rectangle.
    let scale = f64::from(bounds.area()) / total;
    let areas: Vec<f64> = weights.iter().map(|&w| w as f64 * scale).collect();

    let mut free = bounds;
    let mut start = 0;
    while start < areas.len() && areas[start] > 0.0 {
        let side = f64::from(free.width.min(free.height));
        // Grow the current row while doing so improves the worst aspect ratio.
        let mut end = start + 1;
        while end < areas.len()
            && areas[end] > 0.0
            && worst(&areas[start..=end], side) <= worst(&areas[start..end], side)
        {
            end += 1;
        }
        free = lay_row(&areas[start..end], free, &mut out[start..end]);
        start = end;
    }
    out
}

/// The worst (largest) aspect ratio in a row laid along a side of length `side`.
fn worst(row: &[f64], side: f64) -> f64 {
    let sum: f64 = row.iter().sum();
    let (min, max) = row.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &a| (lo.min(a), hi.max(a)));
    let side2 = side * side;
    let sum2 = sum * sum;
    (side2 * max / sum2).max(sum2 / (side2 * min))
}

/// Places one row along the shorter side of `free` and returns what's left.
fn lay_row(row: &[f64], free: Rect, out: &mut [Rect]) -> Rect {
    let sum: f64 = row.iter().sum();
    let horizontal = free.width >= free.height; // stack the row vertically on the left
    let length = f64::from(if horizontal { free.height } else { free.width });
    let thickness = (sum / length) as f32;

    let mut offset = 0.0f32;
    for (rect, &area) in out.iter_mut().zip(row) {
        let extent = (area / f64::from(thickness)) as f32;
        *rect = if horizontal {
            Rect::new(free.x, free.y + offset, thickness, extent)
        } else {
            Rect::new(free.x + offset, free.y, extent, thickness)
        };
        offset += extent;
    }

    if horizontal {
        Rect::new(free.x + thickness, free.y, (free.width - thickness).max(0.0), free.height)
    } else {
        Rect::new(free.x, free.y + thickness, free.width, (free.height - thickness).max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: Rect = Rect::new(0.0, 0.0, 600.0, 400.0);

    #[test]
    fn areas_are_proportional_to_weights() {
        let weights = [6, 6, 4, 3, 2, 2, 1];
        let rects = squarify(&weights, BOUNDS);
        let total: u64 = weights.iter().sum();
        for (w, r) in weights.iter().zip(&rects) {
            let expected = BOUNDS.area() * *w as f32 / total as f32;
            assert!((r.area() - expected).abs() < 0.5, "{w}: {} vs {expected}", r.area());
        }
    }

    #[test]
    fn rectangles_stay_inside_bounds_and_do_not_overlap() {
        let rects = squarify(&[50, 30, 10, 5, 3, 1, 1], BOUNDS);
        for r in &rects {
            assert!(r.x >= -0.01 && r.y >= -0.01);
            assert!(r.x + r.width <= BOUNDS.width + 0.01);
            assert!(r.y + r.height <= BOUNDS.height + 0.01);
        }
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let overlap_w = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
                let overlap_h = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
                assert!(overlap_w <= 0.01 || overlap_h <= 0.01, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn degenerate_inputs_produce_empty_rectangles() {
        assert!(squarify(&[], BOUNDS).is_empty());
        assert_eq!(squarify(&[0, 0], BOUNDS), vec![Rect::default(); 2]);
        assert_eq!(squarify(&[5], Rect::default()), vec![Rect::default()]);
        let with_zero = squarify(&[5, 0], BOUNDS);
        assert_eq!(with_zero[1], Rect::default());
    }

    #[test]
    fn single_weight_fills_bounds() {
        assert_eq!(squarify(&[42], BOUNDS), vec![BOUNDS]);
    }

    #[test]
    fn hit_testing_and_inset() {
        let r = Rect::new(10.0, 10.0, 20.0, 20.0);
        assert!(r.contains(15.0, 29.9));
        assert!(!r.contains(30.0, 15.0));
        assert_eq!(r.inset(2.0), Rect::new(12.0, 12.0, 16.0, 16.0));
        assert_eq!(r.inset(50.0).area(), 0.0);
    }
}

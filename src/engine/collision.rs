//! Collision helpers for axis-aligned rectangles.

use std::collections::HashMap;

use super::geom::FRect;
use crate::rng;

/// Pushes `rect` back inside `bounds` if it sticks out. True if it had to be pushed (it hit a wall).
pub fn keep_inside(rect: &mut FRect, bounds: &FRect) -> bool {
    if bounds.contains(rect) {
        return false;
    }
    rect.clamp_ip(bounds);
    true
}

/// If the rectangles overlap (or are closer than `gap`), moves them apart along the axis where they overlap less
/// (the shorter way out) until `gap` pixels are between them. The heavier one moves less: with equal weights both
/// move half, against a much heavier one the light one does nearly all of it. A weight of `f64::INFINITY` never
/// moves at all (like a wall). True if they had to be moved.
pub fn push_apart(a: &mut FRect, b: &mut FRect, weight_a: f64, weight_b: f64, gap: f64) -> bool {
    let dx = (b.centerx() - a.centerx()) as f64;
    let overlap_x = (a.w as f64 + b.w as f64) / 2.0 + gap - dx.abs();
    if overlap_x <= 0.0 {
        return false;
    }
    let dy = (b.centery() - a.centery()) as f64;
    let overlap_y = (a.h as f64 + b.h as f64) / 2.0 + gap - dy.abs();
    if overlap_y <= 0.0 {
        return false;
    }
    let (share_a, share_b) = if weight_a.is_infinite() || weight_b.is_infinite() {
        // an immovable one: the other does all of it (two: nobody moves)
        (
            (!weight_a.is_infinite() && weight_b.is_infinite()) as i32 as f64,
            (!weight_b.is_infinite() && weight_a.is_infinite()) as i32 as f64,
        )
    } else {
        let share_a = weight_b / (weight_a + weight_b); // the part of the way a has to go
        (share_a, 1.0 - share_a)
    };
    let sign = |d: f64| {
        if d > 0.0 {
            1.0
        } else if d < 0.0 {
            -1.0
        } else if rng::chance(0.5) {
            1.0 // exactly on top of each other: pick a side
        } else {
            -1.0
        }
    };
    if overlap_x < overlap_y {
        let s = sign(dx);
        a.x -= (s * overlap_x * share_a) as f32;
        b.x += (s * overlap_x * share_b) as f32;
    } else {
        let s = sign(dy);
        a.y -= (s * overlap_y * share_a) as f32;
        b.y += (s * overlap_y * share_b) as f32;
    }
    true
}

/// True if the rectangles are at most `distance` apart (or overlap).
pub fn close(a: &FRect, b: &FRect, distance: f64) -> bool {
    a.colliderect(&b.inflate(2.0 * distance, 2.0 * distance))
}

/// Every pair (by index) of rectangles close enough to touch – without checking each against each (n² pairs).
///
/// Spatial hash: the world is cut into a grid of `cell_size` squares, every rectangle is put into the cell its
/// centre is in. Two can only touch if their cells are neighbours, so only those are compared. `cell_size` must be
/// at least as big as the biggest one (plus the gap, if they are kept apart by one). The pairs come in the order the
/// Python version made them (cells in the order they were first used).
pub fn nearby_pairs(centers: &[(f32, f32)], cell_size: f64) -> Vec<(usize, usize)> {
    let mut order: Vec<(i64, i64)> = vec![];
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, &(x, y)) in centers.iter().enumerate() {
        let cell = ((x as f64 / cell_size).floor() as i64, (y as f64 / cell_size).floor() as i64);
        grid.entry(cell)
            .or_insert_with(|| {
                order.push(cell);
                vec![]
            })
            .push(i);
    }
    let mut pairs = vec![];
    for (cx, cy) in order {
        for &i in &grid[&(cx, cy)] {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(cell) = grid.get(&(cx + dx, cy + dy)) {
                        for &j in cell {
                            if i < j {
                                pairs.push((i, j));
                            }
                        }
                    }
                }
            }
        }
    }
    pairs
}

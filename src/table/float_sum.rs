//! Lane-split floating-point column sums.
//!
//! A sequential IEEE sum is one dependency chain: each addition waits for the
//! previous result, and the compiler may not reassociate floating-point
//! additions. Assigning elements round-robin to independent accumulators fixes
//! a different, still deterministic, summation order whose chains can overlap
//! in the pipeline and map onto vector lanes without fast-math.

/// Number of independent accumulators. Element `i` of the leading
/// `len - len % LANES` elements is added to accumulator `i % LANES`.
pub(super) const LANES: usize = 16;

pub(super) trait LaneFloat: Copy + std::ops::Add<Output = Self> {
    const ZERO: Self;
}

impl LaneFloat for f32 {
    const ZERO: Self = 0.0;
}

impl LaneFloat for f64 {
    const ZERO: Self = 0.0;
}

pub(super) fn sum_required<T: LaneFloat>(values: &[T]) -> T {
    let mut lanes = [T::ZERO; LANES];
    let mut chunks = values.chunks_exact(LANES);
    for chunk in &mut chunks {
        for (lane, &value) in lanes.iter_mut().zip(chunk) {
            *lane = *lane + value;
        }
    }
    combine(lanes, chunks.remainder().iter().copied())
}

pub(super) fn sum_nullable<T: LaneFloat>(values: &[Option<T>]) -> T {
    let mut lanes = [T::ZERO; LANES];
    let mut chunks = values.chunks_exact(LANES);
    for chunk in &mut chunks {
        for (lane, value) in lanes.iter_mut().zip(chunk) {
            // Lanes start at +0.0 and therefore never hold -0.0, so adding +0.0
            // for a null leaves every lane bit-identical to skipping the cell.
            *lane = *lane + value.unwrap_or(T::ZERO);
        }
    }
    combine(lanes, chunks.remainder().iter().flatten().copied())
}

/// Adds the accumulators in lane order, then the trailing elements in row order.
fn combine<T: LaneFloat>(lanes: [T; LANES], tail: impl Iterator<Item = T>) -> T {
    lanes
        .into_iter()
        .chain(tail)
        .fold(T::ZERO, |total, value| total + value)
}

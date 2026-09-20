//! Row heights and column widths.
//!
//! Almost every row on a sheet is the default height, and almost every column
//! the default width, so only the exceptions are stored. What the grid
//! actually needs is not the sizes but two questions answered quickly:
//!
//! - where does row `n` start?
//! - which row is at this many pixels down?
//!
//! Both are answered in logarithmic time by keeping the exceptions sorted with
//! a running total beside them. A million uniform rows cost nothing, and a
//! hundred resized ones cost a binary search.
//!
//! Sizes are in points, the unit a document stores, and are converted to
//! pixels only when something is drawn.

use std::collections::BTreeMap;

/// One row or column whose size is not the default.
#[derive(Clone, Copy, Debug)]
struct Mark {
    index: u32,
    size: f64,
    /// Where this one starts, measured from the beginning of the axis.
    start: f64,
    /// Total difference from default contributed by every mark before this one.
    drift_before: f64,
}

/// The sizes along one axis of a sheet.
#[derive(Clone, Debug)]
pub struct Axis {
    default_size: f64,
    limit: u32,
    exceptions: BTreeMap<u32, f64>,
    /// `exceptions` in order, with running totals. Rebuilt on change, which
    /// only happens at the speed a person drags something.
    marks: Vec<Mark>,
}

impl Axis {
    /// A uniform axis of `limit + 1` entries.
    pub fn new(default_size: f64, limit: u32) -> Self {
        Self {
            default_size: default_size.max(0.0),
            limit,
            exceptions: BTreeMap::new(),
            marks: Vec::new(),
        }
    }

    pub fn default_size(&self) -> f64 {
        self.default_size
    }

    /// Change the size everything unexceptional uses.
    pub fn set_default_size(&mut self, size: f64) {
        self.default_size = size.max(0.0);
        self.rebuild();
    }

    pub fn size_of(&self, index: u32) -> f64 {
        self.exceptions
            .get(&index)
            .copied()
            .unwrap_or(self.default_size)
    }

    /// True when this row or column has been collapsed to nothing.
    pub fn is_hidden(&self, index: u32) -> bool {
        self.size_of(index) <= 0.0
    }

    /// Give one row or column its own size. `None` returns it to the default.
    ///
    /// A negative size is clamped to zero, which is how hiding is expressed.
    pub fn set_size(&mut self, index: u32, size: Option<f64>) {
        if index > self.limit {
            return;
        }
        match size {
            None => {
                self.exceptions.remove(&index);
            }
            Some(size) => {
                self.exceptions.insert(index, size.max(0.0));
            }
        }
        self.rebuild();
    }

    pub fn hide(&mut self, index: u32) {
        self.set_size(index, Some(0.0));
    }

    /// How many rows or columns carry their own size.
    pub fn exception_count(&self) -> usize {
        self.exceptions.len()
    }

    /// Every index with its own size, in order.
    pub fn exceptions(&self) -> impl Iterator<Item = (u32, f64)> + '_ {
        self.exceptions.iter().map(|(i, s)| (*i, *s))
    }

    /// Distance from the start of the axis to the start of `index`.
    pub fn offset_of(&self, index: u32) -> f64 {
        let index = index.min(self.limit.saturating_add(1));
        let uniform = f64::from(index) * self.default_size;
        uniform + self.drift_before(index)
    }

    /// Total difference from uniform contributed by everything before `index`.
    fn drift_before(&self, index: u32) -> f64 {
        match self.marks.binary_search_by_key(&index, |m| m.index) {
            Ok(at) => self.marks[at].drift_before,
            Err(0) => 0.0,
            Err(at) => {
                let previous = &self.marks[at - 1];
                previous.drift_before + previous.size - self.default_size
            }
        }
    }

    /// The whole extent of the axis.
    pub fn total(&self) -> f64 {
        self.offset_of(self.limit) + self.size_of(self.limit)
    }

    /// Which row or column sits at `offset`, and how far into it that is.
    ///
    /// Hidden entries occupy no space, so they are never the answer.
    pub fn index_at(&self, offset: f64) -> (u32, f64) {
        let offset = offset.max(0.0);

        // Everything before the first exception is uniform.
        let at = self.marks.partition_point(|m| m.start <= offset);
        if at == 0 {
            return self.within_uniform(0, 0.0, offset);
        }

        let mark = &self.marks[at - 1];
        if offset < mark.start + mark.size {
            return (mark.index, offset - mark.start);
        }

        // Past that exception, uniform again until the next one.
        self.within_uniform(mark.index.saturating_add(1), mark.start + mark.size, offset)
    }

    /// Resolve a position inside a stretch of default-sized entries.
    fn within_uniform(&self, first_index: u32, first_offset: f64, offset: f64) -> (u32, f64) {
        if self.default_size <= 0.0 {
            // Every unexceptional entry is collapsed, so there is nowhere to
            // land but the first one.
            return (first_index.min(self.limit), 0.0);
        }
        let steps = ((offset - first_offset) / self.default_size)
            .floor()
            .max(0.0);
        let index = (f64::from(first_index) + steps).min(f64::from(self.limit));
        let start = first_offset + steps * self.default_size;
        (index as u32, offset - start)
    }

    /// The first and last index visible in a window starting at `offset`.
    ///
    /// The count is inclusive of a partly visible entry at each end, which is
    /// what a renderer needs.
    pub fn visible_range(&self, offset: f64, extent: f64) -> (u32, u32) {
        let (first, _) = self.index_at(offset);
        let (last, _) = self.index_at(offset + extent.max(0.0));
        (first, last.min(self.limit))
    }

    fn rebuild(&mut self) {
        self.marks.clear();
        self.marks.reserve(self.exceptions.len());
        let mut drift = 0.0;
        for (index, size) in &self.exceptions {
            let start = f64::from(*index) * self.default_size + drift;
            self.marks.push(Mark {
                index: *index,
                size: *size,
                start,
                drift_before: drift,
            });
            drift += size - self.default_size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: u32 = 999;

    fn uniform() -> Axis {
        Axis::new(20.0, LIMIT)
    }

    #[test]
    fn a_uniform_axis_is_plain_multiplication() {
        let axis = uniform();
        assert_eq!(axis.offset_of(0), 0.0);
        assert_eq!(axis.offset_of(1), 20.0);
        assert_eq!(axis.offset_of(10), 200.0);
        assert_eq!(axis.size_of(7), 20.0);
    }

    #[test]
    fn a_resized_entry_moves_everything_after_it() {
        let mut axis = uniform();
        axis.set_size(2, Some(50.0));
        assert_eq!(axis.offset_of(0), 0.0);
        assert_eq!(axis.offset_of(2), 40.0);
        // The one after starts 50 further on, not 20.
        assert_eq!(axis.offset_of(3), 90.0);
        assert_eq!(axis.offset_of(4), 110.0);
    }

    #[test]
    fn several_resized_entries_accumulate() {
        let mut axis = uniform();
        axis.set_size(1, Some(10.0));
        axis.set_size(3, Some(40.0));
        // 20 + 10 + 20 = 50
        assert_eq!(axis.offset_of(3), 50.0);
        // ...plus 40
        assert_eq!(axis.offset_of(4), 90.0);
        assert_eq!(axis.offset_of(5), 110.0);
    }

    #[test]
    fn returning_to_the_default_undoes_the_shift() {
        let mut axis = uniform();
        axis.set_size(2, Some(50.0));
        assert_eq!(axis.offset_of(3), 90.0);
        axis.set_size(2, None);
        assert_eq!(axis.offset_of(3), 60.0);
        assert_eq!(axis.exception_count(), 0);
    }

    #[test]
    fn position_and_index_are_inverses() {
        let mut axis = uniform();
        axis.set_size(2, Some(50.0));
        axis.set_size(7, Some(5.0));
        for index in 0..20u32 {
            if axis.is_hidden(index) {
                continue;
            }
            let start = axis.offset_of(index);
            let (found, into) = axis.index_at(start);
            assert_eq!(found, index, "offset {start} should be index {index}");
            assert!(into.abs() < 1e-9);
            // And a point in the middle lands on the same entry.
            let (middle, _) = axis.index_at(start + axis.size_of(index) / 2.0);
            assert_eq!(middle, index);
        }
    }

    #[test]
    fn a_point_reports_how_far_into_its_entry_it_is() {
        let axis = uniform();
        let (index, into) = axis.index_at(45.0);
        assert_eq!(index, 2);
        assert_eq!(into, 5.0);
    }

    #[test]
    fn a_hidden_entry_takes_no_space_and_is_never_hit() {
        let mut axis = uniform();
        axis.hide(2);
        assert!(axis.is_hidden(2));
        // Rows 2 and 3 now begin at the same place.
        assert_eq!(axis.offset_of(2), 40.0);
        assert_eq!(axis.offset_of(3), 40.0);
        // And a point there belongs to row 3.
        assert_eq!(axis.index_at(40.0).0, 3);
    }

    #[test]
    fn consecutive_hidden_entries_are_all_skipped() {
        let mut axis = uniform();
        axis.hide(2);
        axis.hide(3);
        axis.hide(4);
        assert_eq!(axis.offset_of(5), 40.0);
        assert_eq!(axis.index_at(40.0).0, 5);
    }

    #[test]
    fn a_negative_size_is_treated_as_hidden() {
        let mut axis = uniform();
        axis.set_size(1, Some(-10.0));
        assert_eq!(axis.size_of(1), 0.0);
        assert!(axis.is_hidden(1));
    }

    #[test]
    fn the_total_covers_every_entry() {
        let axis = uniform();
        assert_eq!(axis.total(), f64::from(LIMIT + 1) * 20.0);

        let mut resized = uniform();
        resized.set_size(0, Some(120.0));
        assert_eq!(resized.total(), f64::from(LIMIT + 1) * 20.0 + 100.0);
    }

    #[test]
    fn a_point_past_the_end_lands_on_the_last_entry() {
        let axis = uniform();
        assert_eq!(axis.index_at(1e9).0, LIMIT);
    }

    #[test]
    fn a_negative_point_lands_on_the_first() {
        let axis = uniform();
        assert_eq!(axis.index_at(-50.0), (0, 0.0));
    }

    #[test]
    fn the_visible_range_includes_partly_shown_entries_at_both_ends() {
        let axis = uniform();
        // A window from halfway through entry 1 to halfway through entry 6.
        let (first, last) = axis.visible_range(30.0, 100.0);
        assert_eq!(first, 1);
        assert_eq!(last, 6);
    }

    #[test]
    fn changing_the_default_moves_everything_unexceptional() {
        let mut axis = uniform();
        axis.set_size(1, Some(100.0));
        assert_eq!(axis.offset_of(2), 120.0);
        axis.set_default_size(10.0);
        // Entry 0 is now 10, entry 1 is still its own 100.
        assert_eq!(axis.offset_of(2), 110.0);
    }

    #[test]
    fn a_wholly_collapsed_axis_does_not_divide_by_zero() {
        let mut axis = Axis::new(0.0, LIMIT);
        axis.set_size(5, Some(10.0));
        assert_eq!(axis.total(), 10.0);
        // Everything before the one visible entry is at zero.
        let (index, _) = axis.index_at(0.0);
        assert!(index <= 5);
        assert_eq!(axis.index_at(5.0).0, 5);
    }

    #[test]
    fn an_index_past_the_limit_is_refused() {
        let mut axis = uniform();
        axis.set_size(LIMIT + 1, Some(99.0));
        assert_eq!(axis.exception_count(), 0);
    }

    #[test]
    fn many_exceptions_stay_consistent() {
        let mut axis = uniform();
        for index in (0..200).step_by(3) {
            axis.set_size(index, Some(f64::from(index % 7) + 1.0));
        }
        // Walk the whole thing and check that offsets accumulate exactly.
        let mut running = 0.0;
        for index in 0..200u32 {
            assert!(
                (axis.offset_of(index) - running).abs() < 1e-9,
                "index {index}: expected {running}, got {}",
                axis.offset_of(index)
            );
            running += axis.size_of(index);
        }
    }
}

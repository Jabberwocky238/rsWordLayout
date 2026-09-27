//! Vertical cursor advance and occupied extent, in 1/7200-inch units.
//!
//! A line can advance farther than it occupies, or occupy farther than it
//! advances. A sequence must retain the furthest occupied bottom of every line.

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct VerticalExtent {
    pub(super) advance_fine: i64,
    pub(super) required_fine: i64,
}

impl VerticalExtent {
    pub(super) fn uniform(height_fine: i64) -> Self {
        Self {
            advance_fine: height_fine,
            required_fine: height_fine,
        }
    }

    /// Append an actual successor whose origin is at this extent's cursor.
    ///
    /// A zero-sized successor still requires its origin. Consequently,
    /// `Default` is not a right identity when advance exceeds required extent.
    /// Represent an absent successor separately rather than appending it here.
    pub(super) fn then(self, next: Self) -> Self {
        Self {
            advance_fine: self.advance_fine + next.advance_fine,
            required_fine: self
                .required_fine
                .max(self.advance_fine + next.required_fine),
        }
    }

    /// Shift the following origin without making trailing space occupy a page.
    pub(super) fn with_gap(self, gap_fine: i64) -> Self {
        Self {
            advance_fine: self.advance_fine + gap_fine,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VerticalExtent as Extent;

    fn extent(advance_fine: i64, required_fine: i64) -> Extent {
        Extent {
            advance_fine,
            required_fine,
        }
    }

    #[test]
    fn final_advance_does_not_have_to_fit_after_the_last_occupied_bottom() {
        // The lines occupy [0, 40] and [100, 140]. A 140-unit page is enough;
        // the next cursor position, 200, does not enlarge the occupied extent.
        assert_eq!(extent(100, 40).then(extent(100, 40)), extent(200, 140));
    }

    #[test]
    fn overlapping_lines_keep_their_full_occupied_extent() {
        // The lines occupy [0, 100] and [40, 140], despite advancing only 80.
        assert_eq!(extent(40, 100).then(extent(40, 100)), extent(80, 140));
    }

    #[test]
    fn an_earlier_line_can_extend_beyond_every_later_line() {
        // Later bottoms are 110 and 220, both before the first line's 250.
        let group = extent(100, 250).then(extent(100, 10)).then(extent(10, 20));
        assert_eq!(group, extent(210, 250));
    }

    #[test]
    fn positive_gap_moves_the_successor_but_does_not_occupy_trailing_space() {
        let first = extent(100, 40).with_gap(30);
        assert_eq!(first, extent(130, 40));
        assert_eq!(first.then(extent(100, 40)), extent(230, 170));
    }

    #[test]
    fn negative_gap_preserves_preceding_content_extent() {
        let first = extent(100, 150).with_gap(-80);
        assert_eq!(first, extent(20, 150));
        assert_eq!(first.then(extent(40, 100)), extent(60, 150));
        assert_eq!(first.then(extent(40, 200)), extent(60, 220));
    }

    #[test]
    fn fine_units_are_not_rounded_at_each_line() {
        // Five 195.6-twip lines advance exactly 978 twips. Rounding every line
        // before composing would instead produce 980 twips.
        let line = Extent::uniform(978);
        let group = line.then(line).then(line).then(line).then(line);
        assert_eq!(group, Extent::uniform(4890));
    }

    #[test]
    fn grouping_lines_and_paragraph_gaps_does_not_change_the_result() {
        let a = extent(100, 40).with_gap(30);
        let b = extent(40, 100).with_gap(-20);
        let c = extent(60, 10);
        let d = extent(20, 95);
        let expected = extent(230, 305);
        assert_eq!(a.then(b).then(c).then(d), expected);
        assert_eq!(a.then(b.then(c)).then(d), expected);
        assert_eq!(a.then(b).then(c.then(d)), expected);
        assert_eq!(a.then(b.then(c.then(d))), expected);
    }

    #[test]
    fn zero_sized_actual_successor_still_requires_its_origin() {
        let first = extent(100, 40);
        assert_eq!(first.then(Extent::default()), extent(100, 100));
        assert_eq!(Extent::default().then(first), first);
    }
}

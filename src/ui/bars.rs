//! Column geometry shared by the two bar charts. The daily chart and the
//! hourly precipitation chart draw completely differently — one is a ratatui
//! `BarChart`, the other writes cells itself — but they answer the same two
//! questions first: how wide is a column, and which slice of the series fits.

/// Columns between one bar and the next.
pub(super) const GAP: u16 = 1;

/// How a series is laid out across the width it has been given.
pub(super) struct Columns {
    /// Bar width plus `GAP`. The bar itself is `stride - GAP`.
    pub stride: u16,
    /// How many bars fit at that stride.
    pub capacity: usize,
}

impl Columns {
    /// Lay out `wanted` bars across `width`, widening them if that leaves room
    /// to spare but never past `max_stride` — beyond that they grow fat rather
    /// than informative. Never below `min_stride` either: on a cramped chart it
    /// is better to show fewer bars than to render every one of them as a
    /// spike.
    ///
    /// `wanted` is how many bars the caller would *like* on screen, which is
    /// not always the length of its series. The daily chart wants all 22 days
    /// and passes that. The hourly chart's series is eight days long and never
    /// fits, so passing its length would peg the stride at the minimum on every
    /// terminal however wide; it passes the span it actually wants to show and
    /// spends surplus width on broader bars instead.
    pub fn fit(width: u16, wanted: usize, min_stride: u16, max_stride: u16) -> Self {
        let room = width as usize + GAP as usize;
        let stride = (room / wanted.max(1)).clamp(min_stride as usize, max_stride as usize);

        Self {
            stride: stride as u16,
            capacity: (room / stride).max(1),
        }
    }

    /// Total width the given number of bars occupies, trailing gap removed.
    pub fn width_of(&self, bars: usize) -> u16 {
        (bars * self.stride as usize).saturating_sub(GAP as usize) as u16
    }
}

/// Where a window of `capacity` columns should begin so that `selected` sits
/// inside it: the page that contains it, stopping at the end of the series
/// rather than scrolling past it.
///
/// Paging rather than centring. A centred window keeps the highlight pinned to
/// the middle of the screen and slides every column underneath it, so pressing
/// an arrow reads as the chart reshuffling rather than as moving through it —
/// the one thing the selection is supposed to show you. Within a page the
/// highlight travels and the bars hold still, which is the way round that makes
/// a step legible.
pub(super) fn window_start(selected: usize, capacity: usize, count: usize) -> usize {
    if count <= capacity {
        return 0;
    }
    ((selected / capacity) * capacity).min(count - capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever gets dropped for want of room, the bars that remain must fit
    /// the area they were given — anything wider is silently clipped.
    #[test]
    fn the_visible_bars_always_fit_their_area() {
        for width in 1u16..=250 {
            for count in [1usize, 5, 22, 192] {
                for (min, max) in [(3u16, 4u16), (2, 3)] {
                    let columns = Columns::fit(width, count, min, max);
                    let shown = columns.capacity.min(count);

                    // Narrower than a single bar there is nothing to fit, and
                    // one clipped column is a better answer than none — the
                    // callers guard such sizes long before this.
                    if width < min {
                        continue;
                    }
                    assert!(
                        columns.width_of(shown) <= width,
                        "width {width}, count {count}: {shown} bars need {}",
                        columns.width_of(shown)
                    );
                }
            }
        }
    }

    /// There is always something to draw, however little room there is.
    #[test]
    fn at_least_one_column_always_survives() {
        for width in 0u16..=10 {
            assert!(
                Columns::fit(width, 192, 3, 4).capacity >= 1,
                "width {width}"
            );
        }
    }

    #[test]
    fn the_stride_stays_within_its_bounds() {
        for width in 1u16..=250 {
            for count in [1usize, 22, 192] {
                let columns = Columns::fit(width, count, 3, 4);
                assert!((3..=4).contains(&columns.stride), "width {width}");
                assert!(columns.capacity >= 1);
            }
        }
    }

    /// A series that fits needs no window at all, however the selection moves.
    #[test]
    fn a_series_that_fits_is_never_scrolled() {
        for selected in 0..10 {
            assert_eq!(window_start(selected, 20, 10), 0);
        }
    }

    #[test]
    fn the_window_holds_still_across_a_page() {
        // 100 items, 20 visible: everything in 40..60 shares one page.
        for selected in 40..60 {
            assert_eq!(window_start(selected, 20, 100), 40, "selected {selected}");
        }
        assert_eq!(
            window_start(39, 20, 100),
            20,
            "and 39 is on the page before"
        );
    }

    /// The bug this replaced centring to fix. A centred window pins the
    /// highlight to the middle of the screen and slides the columns underneath
    /// it, so stepping looks like the chart reshuffling and the selection
    /// appears not to move at all. Its offset within the window has to change.
    #[test]
    fn the_selection_travels_across_the_window_rather_than_the_window_under_it() {
        // A day's step through the hourly chart at a typical terminal width.
        let (capacity, count, step) = (29usize, 173usize, 24usize);

        let offsets: Vec<usize> = (0..7)
            .map(|i| {
                let selected = i * step;
                selected - window_start(selected, capacity, count)
            })
            .collect();

        assert!(
            offsets.windows(2).all(|pair| pair[0] != pair[1]),
            "the highlight sat still while the chart moved: {offsets:?}"
        );

        // And the same for stepping an hour at a time.
        let offsets: Vec<usize> = (0..10)
            .map(|selected| selected - window_start(selected, capacity, count))
            .collect();
        assert!(
            offsets.windows(2).all(|pair| pair[0] != pair[1]),
            "hour steps left the highlight still: {offsets:?}"
        );
    }

    /// Both ends pin rather than scrolling past, so the first and last
    /// screenfuls are stable while arrowing.
    #[test]
    fn the_window_stops_at_both_ends() {
        assert_eq!(window_start(0, 20, 100), 0);
        assert_eq!(window_start(99, 20, 100), 80);
        assert_eq!(window_start(95, 20, 100), 80);
    }

    /// However the three move, the selection must land inside the window —
    /// otherwise arrowing scrolls to somewhere the highlight is not.
    #[test]
    fn the_selection_is_always_inside_the_window() {
        for count in [1usize, 7, 20, 192] {
            for capacity in 1..=40usize {
                for selected in 0..count {
                    let start = window_start(selected, capacity, count);
                    let shown = capacity.min(count);
                    assert!(
                        selected >= start && selected < start + shown,
                        "count {count}, capacity {capacity}, selected {selected} \
                         fell outside {start}..{}",
                        start + shown
                    );
                }
            }
        }
    }
}

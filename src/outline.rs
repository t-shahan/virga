//! The coastlines, borders and lakes drawn under the radar.
//!
//! Rain on a blank pane is a shape with no place. The outline is what says
//! the band is over the bay and not over the town. It is Natural Earth's
//! 1:50m data, which is in the public domain, packed by
//! `scripts/build-outline.py` into the 200 kilobytes embedded here, so the
//! map costs no request and works for any place on the globe.

use crate::weather::radar::Radar;

const DATA: &[u8] = include_bytes!("outline.bin");

/// Degrees per unit of the packed coordinates. See the packing script.
const STEP: f64 = 0.02;

/// How far outside the frame a point may be and still be kept, in degrees.
/// A line is cut where it leaves this margin, and the cut has to happen
/// off screen for the line to reach the frame's edge.
const MARGIN: f64 = 0.5;

/// The outline around a radar's frames, as lines of frame pixels. Points
/// can lie a little outside the frame; whoever draws them clips.
pub fn traced_on(radar: &Radar) -> Vec<Vec<(f64, f64)>> {
    let (south, west, north, east) = radar.bounds();
    let mid = (west + east) / 2.0;
    let is_near = |lat: f64, lon: f64| {
        // Compared the short way round, as `Radar::pixel_of` projects.
        let across = (lon - mid + 180.0).rem_euclid(360.0) - 180.0;
        (south - MARGIN..=north + MARGIN).contains(&lat)
            && across.abs() <= (east - west) / 2.0 + MARGIN
    };

    let mut traced = Vec::new();
    for line in Lines::new(DATA) {
        let mut run: Vec<(f64, f64)> = Vec::new();
        for (lat, lon) in line {
            if is_near(lat, lon) {
                run.push(radar.pixel_of(lat, lon));
            } else if run.len() > 1 {
                traced.push(std::mem::take(&mut run));
            } else {
                run.clear();
            }
        }
        if run.len() > 1 {
            traced.push(run);
        }
    }
    traced
}

/// The packed lines, one after another until the bytes run out. A record
/// that claims more points than remain ends the iteration: the data is
/// built into the binary, and a short read means a bad build, which should
/// cost the map and not the terminal.
struct Lines<'a> {
    rest: &'a [u8],
}

impl<'a> Lines<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { rest: data }
    }
}

/// Bytes before a line's steps: its point count and its first point.
const HEADER: usize = 6;

impl Iterator for Lines<'_> {
    type Item = Vec<(f64, f64)>;

    fn next(&mut self) -> Option<Self::Item> {
        let (header, body) = self.rest.split_at_checked(HEADER)?;
        let count = usize::from(u16::from_le_bytes([header[0], header[1]]));
        let mut lon = i32::from(i16::from_le_bytes([header[2], header[3]]));
        let mut lat = i32::from(i16::from_le_bytes([header[4], header[5]]));
        let (steps, rest) = body.split_at_checked(count.checked_sub(1)? * 2)?;
        self.rest = rest;

        let degrees = |lat: i32, lon: i32| (f64::from(lat) * STEP, f64::from(lon) * STEP);
        let mut points = Vec::with_capacity(count);
        points.push(degrees(lat, lon));
        for &[east, north] in steps.as_chunks::<2>().0 {
            lon += i32::from(east as i8);
            lat += i32::from(north as i8);
            points.push(degrees(lat, lon));
        }
        Some(points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::radar::FRAME_SIDE;

    #[test]
    fn the_embedded_outline_unpacks_whole_and_stays_on_the_globe() {
        let mut lines = Lines::new(DATA);
        let mut points = 0;
        for line in &mut lines {
            assert!(line.len() >= 2);
            for (lat, lon) in line {
                assert!((-90.01..=90.01).contains(&lat), "{lat}");
                assert!((-180.01..=180.01).contains(&lon), "{lon}");
                points += 1;
            }
        }
        assert!(lines.rest.is_empty(), "no bytes left over");
        assert!(points > 50_000, "{points}");
    }

    #[test]
    fn a_line_is_its_first_point_and_the_steps_from_it() {
        let packed = [
            3, 0, // three points
            100, 0, // longitude 2.00
            0xCE, 0xFF, // latitude -1.00
            5, 0xFB, // +0.10, -0.10
            0x81, 127, // -2.54, +2.54
        ];
        let lines: Vec<_> = Lines::new(&packed).collect();
        let close =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;

        assert_eq!(lines.len(), 1);
        assert!(close(lines[0][0], (-1.0, 2.0)), "{:?}", lines[0]);
        assert!(close(lines[0][1], (-1.1, 2.1)), "{:?}", lines[0]);
        assert!(close(lines[0][2], (1.44, -0.44)), "{:?}", lines[0]);
    }

    #[test]
    fn truncated_data_ends_quietly() {
        let whole = [2, 0, 0, 0, 0, 0, 1, 1];
        for cut in 0..whole.len() {
            assert_eq!(Lines::new(&whole[..cut]).count(), 0, "cut at {cut}");
        }
        // A count of zero would underflow the step count.
        assert_eq!(Lines::new(&[0, 0, 0, 0, 0, 0]).count(), 0);
    }

    /// Around Copenhagen there is coast on every side, and what is traced
    /// is in or just outside the frame, not the whole of Europe.
    #[test]
    fn the_outline_around_a_coastal_place_lands_in_its_frame() {
        let radar = Radar::fixture(55.7, 12.6, Vec::new());
        let traced = traced_on(&radar);
        let side = FRAME_SIDE as f64;

        let inside = traced
            .iter()
            .flatten()
            .filter(|(x, y)| (0.0..side).contains(x) && (0.0..side).contains(y))
            .count();
        assert!(inside > 200, "{inside} points in frame");
        for &(x, y) in traced.iter().flatten() {
            assert!((-80.0..side + 80.0).contains(&x), "{x}");
            assert!((-80.0..side + 80.0).contains(&y), "{y}");
        }
    }

    #[test]
    fn the_open_ocean_has_no_outline() {
        let radar = Radar::fixture(-40.0, -130.0, Vec::new());
        assert!(traced_on(&radar).is_empty());
    }

    /// Taveuni straddles 180°. A frame centred just west of it still gets
    /// the coast that is stored on the east side.
    #[test]
    fn the_outline_follows_a_frame_across_the_antimeridian() {
        let radar = Radar::fixture(-16.8, 179.9, Vec::new());
        let side = FRAME_SIDE as f64;
        let right_half = traced_on(&radar)
            .iter()
            .flatten()
            .filter(|(x, y)| (side / 2.0..side).contains(x) && (0.0..side).contains(y))
            .count();
        assert!(right_half > 0);
    }
}

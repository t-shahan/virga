//! The coastlines, borders and lakes drawn under the radar.
//!
//! Rain on a blank pane is a shape with no place. The outline is what says
//! the band is over the bay and not over the town. It is Natural Earth's
//! data, which is in the public domain, packed by `scripts/build-outline.py`
//! into the 260 kilobytes embedded here, so the map costs no request and
//! works for any place on the globe.

use crate::weather::radar::Radar;

const DATA: &[u8] = include_bytes!("outline.bin");

/// Degrees per unit of the packed coordinates. See the packing script.
const STEP: f64 = 0.02;

/// How far outside the frame a point may be and still be kept, in degrees.
/// A line is cut where it leaves this margin, and the cut has to happen
/// off screen for the line to reach the frame's edge.
const MARGIN: f64 = 0.5;

/// The zoom from which the 1:50m lines are drawn. Further out, a frame
/// holds so much of them that several points land in every dot.
const FINE_FROM: u32 = 4;

/// The zoom from which states and provinces are drawn. Further out they
/// crowd a continent until the countries cannot be told apart.
const STATES_FROM: u32 = 4;

/// What a line marks. The numbers are the packing script's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Coast,
    Country,
    State,
    Lake,
}

impl Layer {
    fn of(tag: u8) -> Option<Self> {
        match tag & 0x0F {
            0 => Some(Self::Coast),
            1 => Some(Self::Country),
            2 => Some(Self::State),
            3 => Some(Self::Lake),
            _ => None,
        }
    }

    fn is_shown_at(self, zoom: u32) -> bool {
        self != Self::State || zoom >= STATES_FROM
    }

    /// A line that is drawn quietly, so that it is not taken for the edge
    /// of a country or of the land.
    pub fn is_minor(self) -> bool {
        self == Self::State
    }
}

/// Which of the two packed scales a line belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Detail {
    Coarse,
    Fine,
}

impl Detail {
    fn of(tag: u8) -> Option<Self> {
        match tag >> 4 {
            0 => Some(Self::Coarse),
            1 => Some(Self::Fine),
            _ => None,
        }
    }

    fn at(zoom: u32) -> Self {
        if zoom >= FINE_FROM {
            Self::Fine
        } else {
            Self::Coarse
        }
    }
}

/// A stretch of one line, as frame pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Trace {
    pub layer: Layer,
    pub points: Vec<(f64, f64)>,
}

/// The outline around a radar's frames. Points can lie a little outside
/// the frame; whoever draws them clips.
pub fn traced_on(radar: &Radar) -> Vec<Trace> {
    let (south, west, north, east) = radar.bounds();
    let mid = (west + east) / 2.0;
    let is_near = |lat: f64, lon: f64| {
        // Compared the short way round, as `Radar::pixel_of` projects.
        let across = (lon - mid + 180.0).rem_euclid(360.0) - 180.0;
        (south - MARGIN..=north + MARGIN).contains(&lat)
            && across.abs() <= (east - west) / 2.0 + MARGIN
    };

    let mut traced = Vec::new();
    for line in lines_at(radar.zoom()) {
        let layer = line.layer;
        let mut keep = |run: &mut Vec<(f64, f64)>| {
            let points = std::mem::take(run);
            if points.len() > 1 {
                traced.push(Trace { layer, points });
            }
        };
        let mut run = Vec::new();
        for (lat, lon) in line.points() {
            if is_near(lat, lon) {
                run.push(radar.pixel_of(lat, lon));
            } else {
                keep(&mut run);
            }
        }
        keep(&mut run);
    }
    traced
}

/// The lines a map at `zoom` draws: one scale of the two, without the
/// layers that zoom is too far out for.
fn lines_at(zoom: u32) -> impl Iterator<Item = Packed<'static>> {
    let detail = Detail::at(zoom);
    Lines::new(DATA).filter(move |line| line.detail == detail && line.layer.is_shown_at(zoom))
}

/// One line as it is stored. Unpacking is put off until the line is known
/// to be wanted, because most of the file is the scale not being drawn.
struct Packed<'a> {
    layer: Layer,
    detail: Detail,
    first: (i32, i32),
    steps: &'a [u8],
}

impl Packed<'_> {
    /// The line's points, as latitude and longitude in degrees.
    fn points(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        let degrees = |(lon, lat): (i32, i32)| (f64::from(lat) * STEP, f64::from(lon) * STEP);
        let later = self
            .steps
            .as_chunks::<2>()
            .0
            .iter()
            .scan(self.first, |at, &[east, north]| {
                at.0 += i32::from(east as i8);
                at.1 += i32::from(north as i8);
                Some(*at)
            });
        std::iter::once(self.first).chain(later).map(degrees)
    }
}

/// The packed lines, one after another until the bytes run out. A record
/// that claims more points than remain, or is tagged as something this
/// build does not know, ends the iteration: the data is built into the
/// binary, and a bad record means a bad build, which should cost the map
/// and not the terminal.
struct Lines<'a> {
    rest: &'a [u8],
}

impl<'a> Lines<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { rest: data }
    }
}

/// Bytes before a line's steps: its tag, its point count and its first
/// point.
const HEADER: usize = 7;

impl<'a> Iterator for Lines<'a> {
    type Item = Packed<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let (header, body) = self.rest.split_at_checked(HEADER)?;
        let layer = Layer::of(header[0])?;
        let detail = Detail::of(header[0])?;
        let count = usize::from(u16::from_le_bytes([header[1], header[2]]));
        let lon = i32::from(i16::from_le_bytes([header[3], header[4]]));
        let lat = i32::from(i16::from_le_bytes([header[5], header[6]]));
        let (steps, rest) = body.split_at_checked(count.checked_sub(1)? * 2)?;
        self.rest = rest;
        Some(Packed {
            layer,
            detail,
            first: (lon, lat),
            steps,
        })
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
            assert!(line.points().count() >= 2);
            for (lat, lon) in line.points() {
                assert!((-90.01..=90.01).contains(&lat), "{lat}");
                assert!((-180.01..=180.01).contains(&lon), "{lon}");
                points += 1;
            }
        }
        assert!(lines.rest.is_empty(), "no bytes left over");
        assert!(points > 50_000, "{points}");
    }

    fn points_at(zoom: u32, layer: Layer) -> usize {
        lines_at(zoom)
            .filter(|line| line.layer == layer)
            .map(|line| line.points().count())
            .sum()
    }

    /// A view of the world gets the light outline and no states. Close in
    /// gets the heavy one, with them.
    #[test]
    fn the_zoom_picks_the_scale_and_whether_states_are_drawn() {
        for zoom in 0..FINE_FROM {
            let coast = points_at(zoom, Layer::Coast);
            assert!((1_000..12_000).contains(&coast), "{coast} at {zoom}");
            assert!(points_at(zoom, Layer::Country) > 500);
            assert!(points_at(zoom, Layer::Lake) > 50);
            assert_eq!(points_at(zoom, Layer::State), 0);
        }
        for zoom in FINE_FROM..=7 {
            assert!(points_at(zoom, Layer::Coast) > 40_000);
            assert!(points_at(zoom, Layer::Country) > 5_000);
            assert!(points_at(zoom, Layer::State) > 10_000);
            assert!(points_at(zoom, Layer::Lake) > 1_000);
        }
    }

    #[test]
    fn only_state_lines_are_minor() {
        assert!(Layer::State.is_minor());
        for layer in [Layer::Coast, Layer::Country, Layer::Lake] {
            assert!(!layer.is_minor());
            assert!(layer.is_shown_at(0));
        }
        assert!(!Layer::State.is_shown_at(STATES_FROM - 1));
        assert!(Layer::State.is_shown_at(STATES_FROM));
    }

    /// Where Colorado, Kansas, Oklahoma and New Mexico come together there
    /// is no coast, no country's edge and no lake of any size: every line
    /// in the frame is a state's.
    #[test]
    fn inland_america_is_outlined_by_its_states() {
        let traced = traced_on(&Radar::fixture(37.0, -102.5, Vec::new()));
        assert!(traced.len() >= 3, "{} lines", traced.len());
        assert!(traced.iter().all(|trace| trace.layer == Layer::State));
    }

    #[test]
    fn a_coast_and_a_border_keep_their_layers() {
        // Geneva: the lake, and France on three sides.
        let traced = traced_on(&Radar::fixture(46.2, 6.15, Vec::new()));
        for layer in [Layer::Country, Layer::Lake] {
            assert!(traced.iter().any(|trace| trace.layer == layer), "{layer:?}");
        }
    }

    #[test]
    fn a_line_is_its_first_point_and_the_steps_from_it() {
        let packed = [
            0x12, // a state line, fine
            3, 0, // three points
            100, 0, // longitude 2.00
            0xCE, 0xFF, // latitude -1.00
            5, 0xFB, // +0.10, -0.10
            0x81, 127, // -2.54, +2.54
        ];
        let lines: Vec<Vec<_>> = Lines::new(&packed)
            .inspect(|line| {
                assert_eq!((line.layer, line.detail), (Layer::State, Detail::Fine));
            })
            .map(|line| line.points().collect())
            .collect();
        let close =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;

        assert_eq!(lines.len(), 1);
        assert!(close(lines[0][0], (-1.0, 2.0)), "{:?}", lines[0]);
        assert!(close(lines[0][1], (-1.1, 2.1)), "{:?}", lines[0]);
        assert!(close(lines[0][2], (1.44, -0.44)), "{:?}", lines[0]);
    }

    #[test]
    fn truncated_data_ends_quietly() {
        let whole = [0, 2, 0, 0, 0, 0, 0, 1, 1];
        assert_eq!(Lines::new(&whole).count(), 1);
        for cut in 0..whole.len() {
            assert_eq!(Lines::new(&whole[..cut]).count(), 0, "cut at {cut}");
        }
        // A count of zero would underflow the step count.
        assert_eq!(Lines::new(&[0, 0, 0, 0, 0, 0, 0]).count(), 0);
    }

    /// A layer or a scale this build has no name for stops the reading
    /// there, and what came before it is kept.
    #[test]
    fn an_unknown_tag_ends_the_outline_without_a_panic() {
        let good = [0x10, 2, 0, 0, 0, 0, 0, 1, 1];
        for tag in [0x04, 0x0F, 0x20, 0xFF] {
            let mut data = good.to_vec();
            data.extend([tag, 2, 0, 0, 0, 0, 0, 1, 1]);
            data.extend(good);
            assert_eq!(Lines::new(&data).count(), 1, "tag {tag:#04x}");
        }
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
            .flat_map(|trace| &trace.points)
            .filter(|(x, y)| (0.0..side).contains(x) && (0.0..side).contains(y))
            .count();
        assert!(inside > 200, "{inside} points in frame");
        for &(x, y) in traced.iter().flat_map(|trace| &trace.points) {
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
            .flat_map(|trace| &trace.points)
            .filter(|(x, y)| (side / 2.0..side).contains(x) && (0.0..side).contains(y))
            .count();
        assert!(right_half > 0);
    }
}

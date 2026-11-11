//! The last two hours of radar around a place, from RainViewer.
//!
//! RainViewer serves a picture, not readings: a PNG of the area around a
//! point, already coloured. Each frame is reduced here to a grid of
//! `Level`s, so the screen draws intensity in the theme's own colours and
//! no pixel colour travels further than this file.

use crate::weather::client::{Endpoints, agent, read_bounded, reply_from, service_at};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::f64::consts::PI;
use std::io::Cursor;
use ureq::Agent;

/// Pixels along each side of a frame, the smaller of the two sizes served.
/// A pane has fewer braille dots than this in either direction on all but
/// the largest terminals, so the larger size would be thrown away.
pub const FRAME_SIDE: usize = 256;

/// Web Mercator zoom. A frame is one tile's worth of the world at this
/// zoom, 5.6 degrees of longitude: about 380 km across at 52°N, which holds
/// what can arrive in the two hours the frames span.
const ZOOM: u32 = 6;

/// The side of the whole world at `ZOOM`, in frame pixels.
const WORLD: f64 = (FRAME_SIDE << ZOOM) as f64;

/// Frames kept. RainViewer lists two hours at ten-minute steps, thirteen in
/// all; the bound is for a list that has grown, not the one expected.
const MAX_FRAMES: usize = 16;

/// The centre of the request is the place rounded to this many degrees,
/// about eleven kilometres. The picture is 380 km wide, so the rounding
/// costs nothing on screen, and RainViewer learns the region and not the
/// address. The marker is still drawn at the place itself.
const CENTER_STEP: f64 = 0.1;

/// "Universal Blue" in RainViewer's numbering, the palette `Level::of`
/// reads back, so this and that function change together. The service
/// currently paints that palette whatever number is sent; naming it means
/// a service that starts honouring the number again changes nothing here.
const COLOR_SCHEME: u8 = 2;

/// How hard it is falling, in the bands the palette makes distinguishable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    None,
    /// The translucent echo below the first solid colour: virga, drizzle,
    /// or the radar seeing something that is not rain.
    Trace,
    Light,
    Moderate,
    Heavy,
    Intense,
}

impl Level {
    /// The band a pixel's colour belongs to. The palette runs from a tan
    /// that grows more opaque, through light blue darkening as the rain
    /// strengthens, to yellow, orange and red. Read by channel order and
    /// not from a table of exact colours, so a smoothed or resampled pixel
    /// between two table entries still lands in a sensible band.
    fn of([red, green, blue, alpha]: [u8; 4]) -> Self {
        /// Light blue darkens to this before the palette calls it moderate.
        const MODERATE_BELOW_BLUE: u8 = 200;
        /// Yellow and orange keep green above this; red does not.
        const INTENSE_BELOW_GREEN: u8 = 120;

        match alpha {
            0 => Self::None,
            1..=254 => Self::Trace,
            255 if blue > red && blue >= MODERATE_BELOW_BLUE => Self::Light,
            255 if blue > red => Self::Moderate,
            255 if green >= INTENSE_BELOW_GREEN => Self::Heavy,
            255 => Self::Intense,
        }
    }
}

/// One moment of radar: when, and a square grid of levels, north at the
/// top, in Web Mercator.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Seconds since the Unix epoch.
    pub time: i64,
    levels: Vec<Level>,
}

impl Frame {
    /// The level at a pixel. Outside the frame nothing is known, which
    /// draws the same as nothing falling.
    pub fn level_at(&self, x: usize, y: usize) -> Level {
        if x >= FRAME_SIDE || y >= FRAME_SIDE {
            return Level::None;
        }
        self.levels
            .get(y * FRAME_SIDE + x)
            .copied()
            .unwrap_or(Level::None)
    }

    /// A PNG as RainViewer sends it. Anything else is an error: a frame of
    /// another size would be drawn at the wrong scale under the coastline,
    /// and a wrong map is worse than a missing frame.
    fn decode(time: i64, png: &[u8]) -> Result<Self> {
        let mut decoder = png::Decoder::new(Cursor::new(png));
        // The decoder allocates from the header's claims. A header is a few
        // bytes and can claim any size, so the bound is on what it may
        // allocate and not on the file.
        decoder.set_limits(png::Limits {
            bytes: 4 * FRAME_SIDE * FRAME_SIDE + 64 * 1024,
        });
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
        let mut reader = decoder.read_info().context("not a PNG")?;

        let (width, height) = reader.info().size();
        ensure!(
            (width as usize, height as usize) == (FRAME_SIDE, FRAME_SIDE),
            "a radar frame of {width}x{height}, not {FRAME_SIDE}x{FRAME_SIDE}"
        );
        let size = reader
            .output_buffer_size()
            .context("a radar frame too large to hold")?;
        let mut pixels = vec![0; size];
        let info = reader
            .next_frame(&mut pixels)
            .context("a damaged radar frame")?;
        ensure!(
            (info.color_type, info.bit_depth) == (png::ColorType::Rgba, png::BitDepth::Eight),
            "a radar frame that is not 8-bit colour"
        );

        let levels: Vec<Level> = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .take(FRAME_SIDE * FRAME_SIDE)
            .map(|&pixel| Level::of(pixel))
            .collect();
        ensure!(
            levels.len() == FRAME_SIDE * FRAME_SIDE,
            "a radar frame that ends early"
        );
        Ok(Self { time, levels })
    }
}

/// The frames for a place, oldest first, and where their shared centre is.
#[derive(Clone, Debug, PartialEq)]
pub struct Radar {
    center_lat: f64,
    center_lon: f64,
    pub frames: Vec<Frame>,
    /// Frames the service listed that did not arrive. The loop is shown
    /// without them, and says so: two frames are not two hours.
    pub missing: usize,
}

impl Radar {
    /// Where a point on the globe falls in a frame, in pixels from its top
    /// left corner. Not clamped: a caller asks about coastlines that leave
    /// the frame and wants to know which way.
    pub fn pixel_of(&self, lat: f64, lon: f64) -> (f64, f64) {
        let (x, y) = world_pixel(lat, lon);
        let (center_x, center_y) = world_pixel(self.center_lat, self.center_lon);
        let half = FRAME_SIDE as f64 / 2.0;
        // The shorter way round, so a place beside the antimeridian is not
        // a world away from the coast on its other side.
        let across = (x - center_x + WORLD / 2.0).rem_euclid(WORLD) - WORLD / 2.0;
        (across + half, y - center_y + half)
    }

    /// The box of latitude and longitude that holds the frame, as
    /// `(south, west, north, east)`, for choosing which coastlines matter.
    /// East can exceed 180 and west fall below -180 beside the antimeridian.
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let (center_x, center_y) = world_pixel(self.center_lat, self.center_lon);
        let half = FRAME_SIDE as f64 / 2.0;
        let lat_at = |y: f64| (PI * (1.0 - 2.0 * y / WORLD)).sinh().atan().to_degrees();
        let lon_at = |x: f64| x / WORLD * 360.0 - 180.0;
        (
            lat_at(center_y + half),
            lon_at(center_x - half),
            lat_at(center_y - half),
            lon_at(center_x + half),
        )
    }

    #[cfg(test)]
    pub fn fixture(center_lat: f64, center_lon: f64, frames: Vec<Frame>) -> Self {
        Self {
            center_lat,
            center_lon,
            frames,
            missing: 0,
        }
    }
}

#[cfg(test)]
impl Frame {
    /// A frame with `level` wherever `wet(x, y)` says so.
    pub fn fixture(time: i64, level: Level, wet: impl Fn(usize, usize) -> bool) -> Self {
        let levels = (0..FRAME_SIDE * FRAME_SIDE)
            .map(|i| {
                if wet(i % FRAME_SIDE, i / FRAME_SIDE) {
                    level
                } else {
                    Level::None
                }
            })
            .collect();
        Self { time, levels }
    }
}

/// Latitudes Web Mercator can show. Past them the projection runs off to
/// infinity; RainViewer's tiles stop there too.
const MERCATOR_LIMIT: f64 = 85.0;

fn world_pixel(lat: f64, lon: f64) -> (f64, f64) {
    let lat = lat.clamp(-MERCATOR_LIMIT, MERCATOR_LIMIT).to_radians();
    let x = (lon + 180.0) / 360.0 * WORLD;
    let y = (1.0 - lat.tan().asinh() / PI) / 2.0 * WORLD;
    (x, y)
}

#[derive(Deserialize)]
struct IndexDto {
    #[serde(default)]
    radar: RadarDto,
}

#[derive(Deserialize, Default)]
struct RadarDto {
    #[serde(default)]
    past: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct FrameDto {
    time: i64,
    path: String,
}

impl IndexDto {
    /// The newest frames the list names, oldest first. An entry of the
    /// wrong shape is dropped and the rest stand. A path is only accepted
    /// in the one form RainViewer uses, because it is about to become part
    /// of a URL and a reply must not be able to aim that anywhere else.
    fn into_frames(self) -> Vec<FrameDto> {
        let mut frames: Vec<FrameDto> = self
            .radar
            .past
            .into_iter()
            .filter_map(|entry| serde_json::from_value::<FrameDto>(entry).ok())
            .filter(|frame| is_frame_path(&frame.path))
            .collect();
        frames.sort_by_key(|frame| frame.time);
        let surplus = frames.len().saturating_sub(MAX_FRAMES);
        frames.drain(..surplus);
        frames
    }
}

fn is_frame_path(path: &str) -> bool {
    const PREFIX: &str = "/v2/radar/";
    const MAX_ID: usize = 64;
    path.strip_prefix(PREFIX).is_some_and(|id| {
        !id.is_empty()
            && id.len() <= MAX_ID
            && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

/// The radar around a place: one request for the list of frames and one
/// for each frame's picture.
pub fn fetch_radar(lat: f64, lon: f64) -> Result<Radar> {
    fetch_radar_with(agent(), &Endpoints::default(), lat, lon)
}

pub(crate) fn fetch_radar_with(
    agent: &Agent,
    endpoints: &Endpoints,
    lat: f64,
    lon: f64,
) -> Result<Radar> {
    let listed = fetch_index_with(agent, endpoints)?;
    ensure!(!listed.is_empty(), "the radar service listed no frames");

    let round = |degrees: f64| (degrees / CENTER_STEP).round() * CENTER_STEP;
    let (center_lat, center_lon) = (
        round(lat).clamp(-MERCATOR_LIMIT, MERCATOR_LIMIT),
        round(lon),
    );

    let frames = gather(listed.len(), |index| {
        let frame = &listed[index];
        let url = format!(
            "{}{}/{FRAME_SIDE}/{ZOOM}/{center_lat:.1}/{center_lon:.1}/{COLOR_SCHEME}/0_0.png",
            endpoints.radar_frames, frame.path
        );
        fetch_frame_with(agent, &url, frame.time)
    })?;
    Ok(Radar {
        center_lat,
        center_lon,
        missing: listed.len() - frames.len(),
        frames,
    })
}

/// A missing frame is a stutter in the loop and not a reason to show
/// nothing, so one failure is forgiven. A second ends the attempt with what
/// has arrived: each failure can cost a full timeout, and a host that is
/// down would otherwise be waited on once per frame.
const FORGIVEN_FRAMES: usize = 1;

/// Frames `0..count`, oldest first, asked for newest first. When the
/// attempt is cut short, what is in hand is then the present and the
/// recent past, which is what the screen is for, and not the far end of
/// two hours ago.
fn gather(count: usize, mut fetch: impl FnMut(usize) -> Result<Frame>) -> Result<Vec<Frame>> {
    let mut frames = Vec::new();
    let mut failures = Vec::new();
    for index in (0..count).rev() {
        match fetch(index) {
            Ok(frame) => frames.push(frame),
            Err(error) => failures.push(error),
        }
        if failures.len() > FORGIVEN_FRAMES {
            break;
        }
    }
    frames.reverse();
    match failures.into_iter().next() {
        Some(first) if frames.is_empty() => Err(first),
        _ => Ok(frames),
    }
}

fn fetch_index_with(agent: &Agent, endpoints: &Endpoints) -> Result<Vec<FrameDto>> {
    let url = &endpoints.radar_index;
    let mut response = agent
        .get(url)
        .call()
        .with_context(|| service_at("radar service", url))?;
    let bytes = read_bounded(&mut response).with_context(|| reply_from("radar service", url))?;
    let index: IndexDto =
        serde_json::from_slice(&bytes).with_context(|| reply_from("radar service", url))?;
    Ok(index.into_frames())
}

fn fetch_frame_with(agent: &Agent, url: &str, time: i64) -> Result<Frame> {
    let mut response = agent
        .get(url)
        .call()
        .with_context(|| service_at("radar service", url))?;
    let bytes = read_bounded(&mut response).with_context(|| reply_from("radar service", url))?;
    if bytes.is_empty() {
        bail!("an empty reply from the radar service at {url}");
    }
    Frame::decode(time, &bytes).with_context(|| reply_from("radar service", url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::client::tests::{serving_response, test_agent};

    const FRAME_PNG: &[u8] = include_bytes!("../../tests/fixtures/radar_frame.png");
    const INDEX_JSON: &str = include_str!("../../tests/fixtures/radar_index.json");

    #[test]
    fn the_palette_reads_back_as_bands_of_intensity() {
        assert_eq!(Level::of([0, 0, 0, 0]), Level::None);
        assert_eq!(Level::of([120, 115, 100, 57]), Level::Trace);
        assert_eq!(Level::of([136, 221, 238, 255]), Level::Light);
        assert_eq!(Level::of([0, 154, 213, 255]), Level::Light);
        assert_eq!(Level::of([0, 127, 180, 255]), Level::Moderate);
        assert_eq!(Level::of([0, 71, 104, 255]), Level::Moderate);
        assert_eq!(Level::of([255, 224, 0, 255]), Level::Heavy);
        assert_eq!(Level::of([255, 159, 0, 255]), Level::Heavy);
        assert_eq!(Level::of([255, 40, 0, 255]), Level::Intense);
        assert_eq!(Level::of([200, 0, 100, 255]), Level::Intense);
    }

    /// A frame RainViewer really sent, read at pixels whose colours were
    /// looked up when it was recorded.
    #[test]
    fn a_recorded_frame_decodes_to_the_levels_it_was_painted_with() {
        let frame = Frame::decode(1_794_418_800, FRAME_PNG).unwrap();

        assert_eq!(frame.time, 1_794_418_800);
        assert_eq!(frame.level_at(0, 0), Level::None);
        assert_eq!(frame.level_at(90, 0), Level::Trace);
        assert_eq!(frame.level_at(145, 24), Level::Light);
        assert_eq!(frame.level_at(75, 52), Level::Moderate);
        assert_eq!(frame.level_at(157, 71), Level::Heavy);
        assert_eq!(frame.level_at(FRAME_SIDE, 0), Level::None, "off the edge");
    }

    #[test]
    fn what_is_not_a_frame_is_refused_without_panicking() {
        for bytes in [&b""[..], b"<html>captive portal</html>", &FRAME_PNG[..200]] {
            assert!(Frame::decode(0, bytes).is_err());
        }

        // A valid PNG of the wrong size: the header alone, claiming a
        // picture far larger than a frame.
        let mut huge = FRAME_PNG[..33].to_vec();
        huge[16..24].copy_from_slice(&[0, 0, 0x40, 0, 0, 0, 0x40, 0]);
        let refused = Frame::decode(0, &huge).unwrap_err();
        assert!(!format!("{refused:#}").is_empty());
    }

    #[test]
    fn the_recorded_list_yields_its_frames_oldest_first() {
        let index: IndexDto = serde_json::from_str(INDEX_JSON).unwrap();
        let frames = index.into_frames();

        assert_eq!(frames.len(), 13);
        assert!(frames.windows(2).all(|pair| pair[0].time < pair[1].time));
        assert_eq!(frames[12].time - frames[0].time, 2 * 60 * 60);
    }

    /// A path becomes part of the next URL. One that would leave the frame
    /// directory, name another host, or carry a query is dropped; so is an
    /// entry that is not an entry at all.
    #[test]
    fn a_list_cannot_aim_the_next_request_somewhere_else() {
        let index: IndexDto = serde_json::from_str(
            r#"{"radar": {"past": [
                {"time": 3, "path": "/v2/radar/abc123"},
                {"time": 1, "path": "/v2/radar/../../secret"},
                {"time": 2, "path": "//evil.example/v2/radar/abc"},
                {"time": 4, "path": "/v2/radar/abc?x=1"},
                {"time": 5, "path": "/v2/radar/"},
                {"time": "soon", "path": "/v2/radar/abc"},
                null,
                {"time": 6, "path": "/v2/radar/nowcast_0f"}
            ]}}"#,
        )
        .unwrap();

        let paths: Vec<String> = index.into_frames().into_iter().map(|f| f.path).collect();
        assert_eq!(paths, ["/v2/radar/abc123", "/v2/radar/nowcast_0f"]);
    }

    #[test]
    fn an_absent_or_misshapen_list_is_no_frames() {
        for body in [
            "{}",
            r#"{"radar": {}}"#,
            r#"{"radar": {"past": []}}"#,
            r#"{"radar": {"nowcast": [1, 2]}}"#,
        ] {
            let index: IndexDto = serde_json::from_str(body).unwrap();
            assert!(index.into_frames().is_empty(), "{body}");
        }
    }

    #[test]
    fn a_long_list_keeps_only_its_newest_frames() {
        let entries: Vec<String> = (0..40)
            .map(|i| format!(r#"{{"time": {i}, "path": "/v2/radar/f{i}"}}"#))
            .collect();
        let body = format!(r#"{{"radar": {{"past": [{}]}}}}"#, entries.join(","));
        let index: IndexDto = serde_json::from_str(&body).unwrap();

        let frames = index.into_frames();
        assert_eq!(frames.len(), MAX_FRAMES);
        assert_eq!(frames.last().map(|f| f.time), Some(39));
    }

    fn dry(time: i64) -> Result<Frame> {
        Ok(Frame::fixture(time, Level::None, |_, _| false))
    }

    #[test]
    fn one_missing_frame_is_skipped_and_a_second_ends_the_attempt() {
        let one_gap = gather(5, |i| if i == 1 { bail!("503") } else { dry(i as i64) }).unwrap();
        assert_eq!(
            one_gap.iter().map(|f| f.time).collect::<Vec<_>>(),
            [0, 2, 3, 4]
        );

        let mut asked = Vec::new();
        let two_gaps = gather(13, |i| {
            asked.push(i);
            if i == 11 || i == 9 {
                bail!("timed out")
            }
            dry(i as i64)
        })
        .unwrap();
        assert_eq!(asked, [12, 11, 10, 9], "newest first, and no further");
        assert_eq!(
            two_gaps.iter().map(|f| f.time).collect::<Vec<_>>(),
            [10, 12],
            "the newest are what is kept, oldest first"
        );
    }

    #[test]
    fn frames_that_all_fail_report_the_first_failure() {
        let mut asked = 0;
        let failed = gather(13, |i| -> Result<Frame> {
            asked += 1;
            bail!("failure {i}")
        })
        .unwrap_err();

        assert_eq!(failed.to_string(), "failure 12");
        assert_eq!(asked, FORGIVEN_FRAMES + 1);
    }

    /// A PNG of the right size and the wrong kind, as a service that
    /// changed its palette to greyscale would send.
    #[test]
    fn a_frame_that_is_not_colour_is_refused() {
        let mut grey = Vec::new();
        {
            let side = FRAME_SIDE as u32;
            let mut encoder = png::Encoder::new(&mut grey, side, side);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&vec![128; FRAME_SIDE * FRAME_SIDE])
                .unwrap();
        }

        let refused = Frame::decode(0, &grey).unwrap_err();
        assert!(
            refused.to_string().contains("not 8-bit colour"),
            "{refused:#}"
        );
    }

    /// The centre of the picture is the centre pixel, east is right and
    /// north is up, and one frame spans the 5.625 degrees of a zoom 6 tile.
    #[test]
    fn a_frame_is_a_mercator_tile_centred_on_the_place() {
        let radar = Radar::fixture(52.5, 13.4, Vec::new());
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;

        let (x, y) = radar.pixel_of(52.5, 13.4);
        assert!(close(x, 128.0) && close(y, 128.0), "{x}, {y}");

        let (east, _) = radar.pixel_of(52.5, 13.4 + 5.625 / 2.0);
        assert!(close(east, 256.0), "{east}");
        let (_, north) = radar.pixel_of(53.5, 13.4);
        assert!(north < 128.0);

        let (south, west, north, east) = radar.bounds();
        assert!(close(east - west, 5.625), "{west}..{east}");
        assert!(south < 52.5 && 52.5 < north);
        let (_, top) = radar.pixel_of(north, 13.4);
        let (_, bottom) = radar.pixel_of(south, 13.4);
        assert!(close(top, 0.0) && close(bottom, 256.0), "{top}, {bottom}");
    }

    /// Fiji's coast east of the antimeridian is a few pixels from a centre
    /// west of it, not a world's width away.
    #[test]
    fn the_antimeridian_is_crossed_the_short_way() {
        let radar = Radar::fixture(-17.0, 179.5, Vec::new());
        let (x, _) = radar.pixel_of(-17.0, -179.5);
        assert!((128.0..256.0).contains(&x), "{x}");
    }

    #[test]
    fn a_polar_place_is_asked_for_at_the_edge_of_the_projection() {
        let radar = Radar::fixture(89.0, 0.0, Vec::new());
        let (x, y) = radar.pixel_of(89.0, 0.0);
        assert!(x.is_finite() && y.is_finite());
    }

    fn png_response() -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            FRAME_PNG.len()
        )
        .into_bytes();
        response.extend_from_slice(FRAME_PNG);
        response
    }

    /// The frame request carries the rounded centre and the path the list
    /// named, and what comes back is decoded.
    #[test]
    fn a_frame_is_fetched_from_the_frame_host_and_decoded() {
        let (endpoints, _, log) = serving_response(png_response());
        let url = format!(
            "{}/v2/radar/abc/256/6/52.5/13.4/0/0_0.png",
            endpoints.radar_frames
        );

        let frame = fetch_frame_with(&test_agent(), &url, 7).unwrap();

        assert_eq!(frame.time, 7);
        assert_eq!(frame.level_at(157, 71), Level::Heavy);
        let asked = log.lock().unwrap().join("\n");
        assert!(
            asked.contains("GET /v2/radar/abc/256/6/52.5/13.4/0/0_0.png"),
            "{asked}"
        );
    }

    /// One server cannot be both the list and the pictures, so it is the
    /// list: the frames then fail to decode and the error says where from.
    #[test]
    fn a_radar_whose_frames_are_not_pictures_fails_naming_the_host() {
        let body = INDEX_JSON;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (endpoints, hits, log) = serving_response(response.into_bytes());

        let failed = fetch_radar_with(&test_agent(), &endpoints, 52.52, 13.405).unwrap_err();

        let shown = format!("{failed:#}");
        assert!(shown.contains(&endpoints.radar_frames), "{shown}");
        // The list, then two frames: the second failure ends the attempt.
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 3);
        let asked = log.lock().unwrap().join("\n");
        assert!(asked.contains("/256/6/52.5/13.4/2/0_0.png"), "{asked}");
        assert!(
            !asked.contains("13.405"),
            "the exact place is not sent: {asked}"
        );
    }

    #[test]
    fn a_list_with_no_frames_is_an_error_and_no_frame_is_asked_for() {
        let body = r#"{"radar": {"past": []}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (endpoints, hits, _) = serving_response(response.into_bytes());

        let failed = fetch_radar_with(&test_agent(), &endpoints, 52.5, 13.4).unwrap_err();

        assert!(failed.to_string().contains("no frames"), "{failed:#}");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// The whole path against the real service, at a test point.
    #[test]
    #[ignore]
    fn real_radar_arrives_as_two_hours_of_frames() {
        let radar = fetch_radar(52.52, 13.41).expect("radar");
        println!("frames: {}", radar.frames.len());
        assert!(radar.frames.len() >= 10);
        assert!(radar.frames.windows(2).all(|p| p[0].time < p[1].time));

        // Somewhere on a frame this size there is nearly always an echo,
        // and the palette puts far more of it in the blues than the reds.
        // All of it reading as heavy would mean the colours had changed.
        let count = |wanted: fn(Level) -> bool| {
            radar
                .frames
                .iter()
                .flat_map(|f| (0..FRAME_SIDE * FRAME_SIDE).map(move |i| (f, i)))
                .filter(|(f, i)| wanted(f.level_at(i % FRAME_SIDE, i / FRAME_SIDE)))
                .count()
        };
        let blues = count(|level| matches!(level, Level::Light | Level::Moderate));
        let reds = count(|level| matches!(level, Level::Heavy | Level::Intense));
        println!("light or moderate: {blues}  heavy or intense: {reds}");
        assert!(reds == 0 || blues > reds);
    }
}

//! The radar screen: two hours of rain moving over a map of the coast.
//!
//! Drawn in braille, eight dots to a cell, so a pane of ordinary size has
//! room for the shape of a front. A cell has one colour, so how hard it is
//! falling is carried twice: by the colour of the cell and by how densely
//! its dots are lit. The second is what a terminal without colour still
//! shows.

use crate::app::{App, Fetch};
use crate::radar_view::RadarScene;
use crate::theme::Palette;
use crate::ui::pane::bottom_titles;
use crate::ui::{TITLE_GUTTER, spinner, title_room, truncate};
use crate::weather::radar::{FRAME_SIDE, Frame as RadarFrame, Level};
use chrono::{DateTime, FixedOffset};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Wrap};

/// RainViewer's terms ask for the credit wherever the data is shown, so it
/// is on the border and is the last thing a narrow pane gives up.
const CREDIT: &str = "RainViewer";

const MARKER: &str = "◉";

pub(super) fn radar_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect) {
    match &app.radar {
        Fetch::Ready(scene) => scene_render(frame, app, scene, palette, area),
        Fetch::Failed(error) => {
            let message = format!("No radar: {error}\n\n[r] tries again");
            notice_render(frame, app, palette, area, &message, palette.error);
        }
        Fetch::Loading | Fetch::Idle => {
            let message = format!("{} fetching radar", spinner(app.tick));
            notice_render(frame, app, palette, area, &message, palette.muted);
        }
    }
}

/// The pane with words where the map would be.
fn notice_render(
    frame: &mut Frame,
    app: &App,
    palette: Palette,
    area: Rect,
    message: &str,
    color: Color,
) {
    let block = pane(&app.location.label, "Radar", palette, area.width);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // A third of the way down, so a one-line spinner and a wrapped error
    // both sit where the eye already is.
    let top = inner.height / 3;
    let text = Rect {
        y: inner.y + top,
        height: inner.height - top,
        ..inner
    };
    frame.render_widget(
        Paragraph::new(message.to_string())
            .fg(color)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        text,
    );
}

fn scene_render(frame: &mut Frame, app: &App, scene: &RadarScene, palette: Palette, area: Rect) {
    let frames = &scene.radar.frames;
    let index = app.playback.frame(frames.len());
    // Missing frames first: with most of the loop absent, "nothing
    // falling" would be a claim about hours nobody has seen.
    let heading = match (scene.radar.missing, scene.has_rain) {
        (0, true) => "Radar".to_string(),
        (0, false) => "Radar · nothing falling in range".to_string(),
        (1, _) => "Radar · 1 frame missing".to_string(),
        (missing, _) => format!("Radar · {missing} frames missing"),
    };
    let mut block = pane(&scene.location.label, &heading, palette, area.width);

    let offset = match &app.weather {
        Fetch::Ready(weather) => weather.utc_offset_secs,
        _ => None,
    };
    let shown = frames.get(index);
    let caption = shown.map_or_else(Vec::new, |shown| {
        captions(
            shown.time,
            offset,
            index,
            frames.len(),
            app.playback.is_playing(),
        )
    });
    let (caption, credit) = caption
        .iter()
        .find_map(|text| bottom_titles(text, CREDIT, area.width).0)
        .map_or((None, CREDIT.to_string()), |text| {
            (Some(text), CREDIT.to_string())
        });
    block = block.title_bottom(Line::from(credit).fg(palette.muted).right_aligned());
    if let Some(caption) = caption {
        block = block.title_bottom(Line::from(caption).fg(palette.text).left_aligned());
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(shown) = shown else { return };

    let dots = Dots::of(inner);
    let mut cells = vec![Cell::default(); inner.width as usize * inner.height as usize];
    rain_plot(&mut cells, dots, shown);
    for line in &scene.coast {
        coast_plot(&mut cells, dots, line);
    }
    cells_render(frame, inner, &cells, palette);

    let place = scene.radar.pixel_of(scene.location.lat, scene.location.lon);
    marker_render(frame, inner, dots, place, palette);
}

/// The border: the city in its corner as on the weather screen, and what
/// the pane is opposite it when there is room for both.
fn pane(city: &str, heading: &str, palette: Palette, width: u16) -> Block<'static> {
    let available = title_room(width);
    let city = truncate(&city.to_uppercase(), available);
    let len = |text: &str| text.chars().count();
    // "Radar" alone when the whole heading will not fit: the first word is
    // the pane's name and the rest is commentary.
    let short = heading.split(' ').next().unwrap_or(heading);
    let heading = [heading, short]
        .into_iter()
        .find(|text| len(&city) + TITLE_GUTTER + len(text) <= available);

    let mut block = Block::bordered()
        .border_style(Style::new().fg(palette.border))
        .title_top(Line::from(city).bold().fg(palette.accent).left_aligned());
    if let Some(heading) = heading {
        block = block.title_top(
            Line::from(heading.to_string())
                .fg(palette.muted)
                .right_aligned(),
        );
    }
    block
}

/// What the bottom border says about the frame on screen, wordiest first:
/// its time, where it sits in the loop, and whether the loop is stopped.
fn captions(
    time: i64,
    utc_offset_secs: Option<i32>,
    index: usize,
    count: usize,
    is_playing: bool,
) -> Vec<String> {
    let clock = clock(time, utc_offset_secs);
    let scrubber: String = (0..count)
        .map(|i| match i.cmp(&index) {
            std::cmp::Ordering::Less => '━',
            std::cmp::Ordering::Equal => '●',
            std::cmp::Ordering::Greater => '─',
        })
        .collect();
    let with_scrubber = format!("{clock} {scrubber}");
    let paused = (!is_playing).then(|| format!("{with_scrubber} paused"));
    paused.into_iter().chain([with_scrubber, clock]).collect()
}

/// The frame's time on the place's own clock, as the rest of the app tells
/// time. Without the place's offset it is given in UTC and says so; a
/// time that will not convert is left out, not guessed.
fn clock(time: i64, utc_offset_secs: Option<i32>) -> String {
    let Some(at) = DateTime::from_timestamp(time, 0) else {
        return String::new();
    };
    match utc_offset_secs.and_then(FixedOffset::east_opt) {
        Some(offset) => at.with_timezone(&offset).format("%-I:%M %p").to_string(),
        None => at.format("%H:%M UTC").to_string(),
    }
}

/// The dot grid a pane offers and how it is laid over a frame.
///
/// The frame is square and a pane rarely is. The longer side of the pane
/// spans the whole frame and the shorter shows a centred band of it, so no
/// dot is left empty and the scale is the same both ways. A braille dot is
/// close enough to square in a terminal's two-to-one cell that north stays
/// north.
#[derive(Clone, Copy)]
struct Dots {
    across: usize,
    down: usize,
    /// Frame pixels per dot.
    scale: f64,
}

const DOTS_ACROSS_CELL: usize = 2;
const DOTS_DOWN_CELL: usize = 4;
const HALF_FRAME: f64 = FRAME_SIDE as f64 / 2.0;

impl Dots {
    fn of(inner: Rect) -> Self {
        let across = inner.width as usize * DOTS_ACROSS_CELL;
        let down = inner.height as usize * DOTS_DOWN_CELL;
        Self {
            across,
            down,
            scale: FRAME_SIDE as f64 / across.max(down).max(1) as f64,
        }
    }

    /// The frame pixel under a dot position. Fractional both ways: a dot's
    /// left edge is `x`, its right edge `x + 1`.
    fn pixel(self, x: f64, y: f64) -> (f64, f64) {
        (
            HALF_FRAME + (x - self.across as f64 / 2.0) * self.scale,
            HALF_FRAME + (y - self.down as f64 / 2.0) * self.scale,
        )
    }

    fn dot(self, (pixel_x, pixel_y): (f64, f64)) -> (f64, f64) {
        (
            (pixel_x - HALF_FRAME) / self.scale + self.across as f64 / 2.0,
            (pixel_y - HALF_FRAME) / self.scale + self.down as f64 / 2.0,
        )
    }

    /// The cell and the bit within it for a dot, or `None` off the grid.
    fn bit(self, x: f64, y: f64) -> Option<(usize, u8)> {
        if !(0.0..self.across as f64).contains(&x) || !(0.0..self.down as f64).contains(&y) {
            return None;
        }
        let (x, y) = (x as usize, y as usize);
        let cell = y / DOTS_DOWN_CELL * (self.across / DOTS_ACROSS_CELL) + x / DOTS_ACROSS_CELL;
        Some((cell, BRAILLE_BITS[y % DOTS_DOWN_CELL][x % DOTS_ACROSS_CELL]))
    }
}

/// Unicode's braille block numbers its dots down the left column and then
/// the right, with the bottom row added later and out of order.
const BRAILLE_BITS: [[u8; DOTS_ACROSS_CELL]; DOTS_DOWN_CELL] =
    [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
const BRAILLE_BLANK: u32 = 0x2800;

#[derive(Clone, Copy)]
struct Cell {
    rain: u8,
    coast: u8,
    level: Level,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            rain: 0,
            coast: 0,
            level: Level::None,
        }
    }
}

/// Whether a dot under rain of `level` is lit. A trace lights one dot in
/// four and light rain every other one, staggered so neither forms stripes;
/// anything heavier is solid.
fn is_lit(level: Level, x: usize, y: usize) -> bool {
    match level {
        Level::None => false,
        Level::Trace => y.is_multiple_of(2) && (x + y / 2).is_multiple_of(2),
        Level::Light => (x + y).is_multiple_of(2),
        Level::Moderate | Level::Heavy | Level::Intense => true,
    }
}

fn rain_plot(cells: &mut [Cell], dots: Dots, frame: &RadarFrame) {
    for y in 0..dots.down {
        for x in 0..dots.across {
            let level = heaviest_under(dots, frame, x, y);
            let Some((cell, bit)) = dots.bit(x as f64, y as f64) else {
                continue;
            };
            let Some(cell) = cells.get_mut(cell) else {
                continue;
            };
            cell.level = cell.level.max(level);
            if is_lit(level, x, y) {
                cell.rain |= bit;
            }
        }
    }
}

/// The heaviest level among the frame pixels a dot covers. A dot usually
/// covers more than one, and taking only the nearest would let a narrow
/// line of storms fall between dots and vanish.
fn heaviest_under(dots: Dots, frame: &RadarFrame, x: usize, y: usize) -> Level {
    let (left, top) = dots.pixel(x as f64, y as f64);
    let (right, bottom) = dots.pixel(x as f64 + 1.0, y as f64 + 1.0);
    let span = |from: f64, to: f64| {
        let first = from.floor().max(0.0) as usize;
        let last = (to.ceil() as usize).saturating_sub(1).max(first);
        first..=last.min(FRAME_SIDE - 1)
    };
    span(top, bottom)
        .flat_map(|pixel_y| span(left, right).map(move |pixel_x| (pixel_x, pixel_y)))
        .map(|(pixel_x, pixel_y)| frame.level_at(pixel_x, pixel_y))
        .max()
        .unwrap_or(Level::None)
}

/// Dots a single stretch of coast may light. A stretch is short, because
/// the outline is cut off a little outside the frame; the bound is for the
/// stretch that is not, so one bad line cannot stall a draw.
const MAX_STEPS: usize = 4096;

fn coast_plot(cells: &mut [Cell], dots: Dots, line: &[(f64, f64)]) {
    for pair in line.windows(2) {
        let (from_x, from_y) = dots.dot(pair[0]);
        let (to_x, to_y) = dots.dot(pair[1]);
        let reach = (to_x - from_x).abs().max((to_y - from_y).abs());
        if !reach.is_finite() {
            continue;
        }
        let steps = (reach.ceil() as usize).clamp(1, MAX_STEPS);
        for step in 0..=steps {
            let along = step as f64 / steps as f64;
            let x = from_x + (to_x - from_x) * along;
            let y = from_y + (to_y - from_y) * along;
            if let Some((cell, bit)) = dots.bit(x, y)
                && let Some(cell) = cells.get_mut(cell)
            {
                cell.coast |= bit;
            }
        }
    }
}

fn cells_render(frame: &mut Frame, inner: Rect, cells: &[Cell], palette: Palette) {
    let buffer = frame.buffer_mut();
    for (index, cell) in cells.iter().enumerate() {
        let Some((bits, style)) = glyph(*cell, palette) else {
            continue;
        };
        let x = inner.x + (index % inner.width as usize) as u16;
        let y = inner.y + (index / inner.width as usize) as u16;
        if let Some(symbol) = char::from_u32(BRAILLE_BLANK + u32::from(bits))
            && let Some(target) = buffer.cell_mut((x, y))
        {
            target.set_char(symbol).set_style(style);
        }
    }
}

/// The dots and colour for a cell, or `None` for one with nothing in it.
///
/// Under a trace, or nothing, the coast is drawn and the haze shares its
/// cell. Under real rain the coast is cut out of the rain instead: lit, it
/// would disappear into a solid cell, and as a gap it stays readable
/// through the heaviest band.
fn glyph(cell: Cell, palette: Palette) -> Option<(u8, Style)> {
    let color = |color: Color| Style::new().fg(color);
    let (bits, style) = match cell.level {
        Level::None | Level::Trace if cell.coast != 0 => {
            (cell.coast | cell.rain, color(palette.border))
        }
        Level::None | Level::Trace => (cell.rain, color(palette.muted)),
        Level::Light | Level::Moderate => (cut(cell), color(palette.accent)),
        Level::Heavy => (cut(cell), color(palette.now)),
        Level::Intense => (cut(cell), color(palette.error).add_modifier(Modifier::BOLD)),
    };
    (bits != 0).then_some((bits, style))
}

/// Rain with the coast's dots taken out, unless that would leave nothing.
fn cut(cell: Cell) -> u8 {
    match cell.rain & !cell.coast {
        0 => cell.rain,
        cut => cut,
    }
}

/// The place itself, over whatever is falling on it.
fn marker_render(frame: &mut Frame, inner: Rect, dots: Dots, place: (f64, f64), palette: Palette) {
    let (x, y) = dots.dot(place);
    if dots.bit(x, y).is_none() {
        return;
    }
    let position = (
        inner.x + (x as usize / DOTS_ACROSS_CELL) as u16,
        inner.y + (y as usize / DOTS_DOWN_CELL) as u16,
    );
    if let Some(target) = frame.buffer_mut().cell_mut(position) {
        target.set_symbol(MARKER).set_style(
            Style::new()
                .fg(palette.selection)
                .add_modifier(Modifier::BOLD),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ActiveLocation, Screen};
    use crate::radar_view::Playback;
    use crate::theme::Theme;
    use crate::weather::model::Weather;
    use crate::weather::radar::Radar;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    fn palette() -> Palette {
        Theme::default().palette()
    }

    fn copenhagen() -> ActiveLocation {
        ActiveLocation {
            label: "Copenhagen, Denmark".to_string(),
            lat: 55.68,
            lon: 12.57,
        }
    }

    /// Two in the afternoon UTC on the day the fixtures were recorded, and
    /// ten minutes on for each frame after.
    const FIRST_FRAME: i64 = 1_794_405_600;

    fn frame_at(index: usize, level: Level, wet: impl Fn(usize, usize) -> bool) -> RadarFrame {
        RadarFrame::fixture(FIRST_FRAME + 600 * index as i64, level, wet)
    }

    /// An app on the radar screen showing `frames` over `place`.
    fn app_showing(place: ActiveLocation, frames: Vec<RadarFrame>) -> App {
        let mut app = App::new();
        let mut weather = Weather::fixture(3, 1);
        weather.utc_offset_secs = Some(3600);
        app.weather = Fetch::Ready(weather);
        app.location = place.clone();
        app.screen = Screen::Radar;
        let radar = Radar::fixture(place.lat, place.lon, frames);
        app.radar = Fetch::Ready(RadarScene::new(place, radar));
        app
    }

    fn drawn(app: &App, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| radar_render(frame, app, palette(), frame.area()))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn row(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect()
    }

    fn is_braille(symbol: &str) -> bool {
        symbol
            .chars()
            .next()
            .is_some_and(|c| ('\u{2801}'..='\u{28FF}').contains(&c))
    }

    /// Cells inside the border that hold `color`'d braille, as (x, y).
    fn braille_in(buffer: &Buffer, color: Color) -> Vec<(u16, u16)> {
        let area = buffer.area;
        (1..area.height - 1)
            .flat_map(|y| (1..area.width - 1).map(move |x| (x, y)))
            .filter(|&at| is_braille(buffer[at].symbol()) && buffer[at].fg == color)
            .collect()
    }

    /// The open ocean, so nothing but rain is on the map.
    fn at_sea() -> ActiveLocation {
        ActiveLocation {
            label: "Point Nemo".to_string(),
            lat: -40.0,
            lon: -130.0,
        }
    }

    /// Rain in the frame's western half is drawn in the pane's left half,
    /// solid and in the rain colour, and nowhere else.
    #[test]
    fn rain_is_drawn_where_the_frame_has_it() {
        let west = frame_at(0, Level::Moderate, |x, _| x < FRAME_SIDE / 2);
        let buffer = drawn(&app_showing(at_sea(), vec![west]), 62, 17);

        let wet = braille_in(&buffer, palette().accent);
        assert!(!wet.is_empty());
        assert!(wet.iter().all(|&(x, _)| x <= 31), "{wet:?}");
        assert!(
            wet.iter().any(|&(x, y)| x == 1 && y == 1),
            "it reaches the corner"
        );
        assert_eq!(buffer[(10, 8)].symbol(), "⣿", "moderate rain is solid");
        assert_eq!(buffer[(50, 8)].symbol(), " ");
    }

    /// The same cell under each level: sparser dots for less, and the
    /// colour stepping up for more.
    #[test]
    fn intensity_is_carried_by_density_and_by_colour() {
        let p = palette();
        let cell_under = |level| {
            let frame = frame_at(0, level, |_, _| true);
            let buffer = drawn(&app_showing(at_sea(), vec![frame]), 62, 17);
            let cell = &buffer[(10, 8)];
            (cell.symbol().to_string(), cell.fg)
        };
        let lit = |symbol: &str| {
            symbol
                .chars()
                .next()
                .map_or(0, |c| (c as u32 - BRAILLE_BLANK).count_ones())
        };

        let (trace, trace_color) = cell_under(Level::Trace);
        let (light, light_color) = cell_under(Level::Light);
        let (moderate, moderate_color) = cell_under(Level::Moderate);
        assert_eq!((lit(&trace), lit(&light), lit(&moderate)), (2, 4, 8));
        assert_eq!(trace_color, p.muted);
        assert_eq!((light_color, moderate_color), (p.accent, p.accent));
        assert_eq!(cell_under(Level::Heavy), ("⣿".to_string(), p.now));
        assert_eq!(cell_under(Level::Intense), ("⣿".to_string(), p.error));
    }

    /// A storm one frame pixel wide still lights a dot when a dot covers
    /// several pixels.
    #[test]
    fn a_narrow_line_of_storms_is_not_lost_between_dots() {
        let line = frame_at(0, Level::Intense, |x, _| x == 77);
        let buffer = drawn(&app_showing(at_sea(), vec![line]), 34, 10);

        assert!(!braille_in(&buffer, palette().error).is_empty());
    }

    #[test]
    fn the_coast_is_drawn_under_a_dry_sky_and_cut_out_of_rain() {
        let dry = frame_at(0, Level::None, |_, _| false);
        let buffer = drawn(&app_showing(copenhagen(), vec![dry]), 80, 24);
        let coast = braille_in(&buffer, palette().border);
        assert!(coast.len() > 40, "{} cells of coast", coast.len());

        let soaked = frame_at(0, Level::Moderate, |_, _| true);
        let wet = drawn(&app_showing(copenhagen(), vec![soaked]), 80, 24);
        let gaps = coast
            .iter()
            .filter(|&&at| wet[at].symbol() != "⣿" && wet[at].fg == palette().accent)
            .count();
        assert!(
            gaps * 10 >= coast.len() * 9,
            "{gaps} of {} cut",
            coast.len()
        );
    }

    #[test]
    fn the_place_is_marked_at_the_centre() {
        let dry = frame_at(0, Level::None, |_, _| false);
        let buffer = drawn(&app_showing(copenhagen(), vec![dry]), 80, 24);

        let marked: Vec<(u16, u16)> = (0..24)
            .flat_map(|y| (0..80).map(move |x| (x, y)))
            .filter(|&at| buffer[at].symbol() == MARKER)
            .collect();
        assert_eq!(marked.len(), 1);
        let (x, y) = marked[0];
        assert!((38..=41).contains(&x) && (11..=12).contains(&y), "{x}, {y}");
        assert_eq!(buffer[(x, y)].fg, palette().selection);
    }

    /// The bottom border tells the frame's time on the place's clock and
    /// where the loop is, and the credit is on it at every width.
    #[test]
    fn the_border_tells_the_time_the_loop_and_the_source() {
        let frames: Vec<_> = (0..13)
            .map(|i| frame_at(i, Level::None, |_, _| false))
            .collect();
        let mut app = app_showing(at_sea(), frames);
        for _ in 0..4 {
            app.playback.step(true, 13);
        }

        let wide = row(&drawn(&app, 80, 20), 19);
        assert!(wide.contains("3:40 PM ━━━━●──────── paused"), "{wide:?}");
        assert!(wide.contains(CREDIT), "{wide:?}");

        let narrow = row(&drawn(&app, 40, 20), 19);
        assert!(narrow.contains("3:40 PM ━━━━●────────"), "{narrow:?}");
        assert!(
            !narrow.contains("paused") && narrow.contains(CREDIT),
            "{narrow:?}"
        );

        let floor = row(&drawn(&app, 34, 12), 11);
        assert!(
            floor.contains("3:40 PM") && floor.contains(CREDIT),
            "{floor:?}"
        );

        app.playback = Playback::default();
        let playing = row(&drawn(&app, 80, 20), 19);
        assert!(playing.contains("3:00 PM ●────────────"), "{playing:?}");
        assert!(!playing.contains("paused"), "{playing:?}");
    }

    #[test]
    fn a_time_without_the_places_offset_is_given_in_utc() {
        assert_eq!(clock(FIRST_FRAME, Some(3600)), "3:00 PM");
        assert_eq!(clock(FIRST_FRAME, None), "14:00 UTC");
        assert_eq!(clock(FIRST_FRAME, Some(i32::MAX)), "14:00 UTC");
        assert_eq!(clock(i64::MAX, Some(0)), "");
    }

    /// An empty map has to say it is empty on purpose.
    #[test]
    fn a_dry_loop_says_nothing_is_falling() {
        let hazy = frame_at(0, Level::Trace, |x, y| x + y < 40);
        let app = app_showing(copenhagen(), vec![hazy]);

        let top = row(&drawn(&app, 80, 20), 0);
        assert!(top.contains("COPENHAGEN, DENMARK"), "{top:?}");
        assert!(top.contains("Radar · nothing falling in range"), "{top:?}");

        let narrow = row(&drawn(&app, 34, 12), 0);
        assert!(
            narrow.contains("Radar") && !narrow.contains("falling"),
            "{narrow:?}"
        );

        let rain = frame_at(0, Level::Light, |x, y| x + y < 40);
        let top = row(&drawn(&app_showing(copenhagen(), vec![rain]), 80, 20), 0);
        assert!(!top.contains("nothing falling"), "{top:?}");
    }

    /// Through the app's own renderer: the map takes the content area and
    /// the key bar under it names the way back.
    #[test]
    fn the_whole_screen_is_the_map_over_its_key_bar() {
        let soaked = frame_at(0, Level::Moderate, |_, _| true);
        let app = app_showing(at_sea(), vec![soaked]);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                crate::ui::render(frame, &app);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(row(&buffer, 0).contains("POINT NEMO"));
        assert_eq!(buffer[(40, 5)].symbol(), "⣿");
        let bar = row(&buffer, 23);
        assert!(
            bar.contains("[b] back") && bar.contains("[q] quit"),
            "{bar:?}"
        );
    }

    /// A loop cut short says how short, in place of any claim about rain.
    #[test]
    fn missing_frames_are_counted_on_the_border() {
        let dry = frame_at(0, Level::None, |_, _| false);
        let mut app = app_showing(copenhagen(), vec![dry]);
        let Fetch::Ready(scene) = &mut app.radar else {
            panic!("a scene")
        };
        scene.radar.missing = 11;

        let top = row(&drawn(&app, 80, 20), 0);
        assert!(top.contains("Radar · 11 frames missing"), "{top:?}");
        assert!(!top.contains("nothing falling"), "{top:?}");

        let Fetch::Ready(scene) = &mut app.radar else {
            panic!("a scene")
        };
        scene.radar.missing = 1;
        let top = row(&drawn(&app, 80, 20), 0);
        assert!(top.contains("Radar · 1 frame missing"), "{top:?}");
    }

    /// The spinner and a long error at the app's floor, in a pane with no
    /// interior, and in one a single cell across.
    #[test]
    fn the_notices_draw_at_awkward_sizes_without_panicking() {
        let mut app = app_showing(copenhagen(), Vec::new());
        for state in [
            Fetch::Loading,
            Fetch::Idle,
            Fetch::Failed("the radar service at https://example.invalid: ".repeat(8)),
        ] {
            app.radar = state;
            for (width, height) in [(34, 12), (34, 3), (3, 3), (2, 2), (1, 1), (200, 4)] {
                let buffer = drawn(&app, width, height);
                assert_eq!(buffer.area.height, height);
            }
        }
    }

    /// The key card over the radar lists the radar's own keys.
    #[test]
    fn the_key_card_names_the_radars_bindings() {
        let dry = frame_at(0, Level::None, |_, _| false);
        let mut app = app_showing(at_sea(), vec![dry]);
        app.help_visible = true;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                crate::ui::render(frame, &app);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = (0..24).map(|y| row(&buffer, y) + "\n").collect();

        for binding in ["[space] pause", "[←→] frame", "[r] refresh", "[b] back"] {
            assert!(text.contains(binding), "{binding} in\n{text}");
        }
        assert!(!text.contains("hourly"), "{text}");
    }

    #[test]
    fn waiting_and_failing_are_said_in_the_pane() {
        let mut app = app_showing(copenhagen(), Vec::new());
        app.radar = Fetch::Loading;
        let waiting = drawn(&app, 60, 15);
        let text: String = (0..15).map(|y| row(&waiting, y)).collect();
        assert!(text.contains("fetching radar"), "{text}");
        assert!(text.contains("COPENHAGEN"), "{text}");

        app.radar = Fetch::Failed("the radar service at https://x: timed out".to_string());
        let failed = drawn(&app, 60, 15);
        let text: String = (0..15).map(|y| row(&failed, y)).collect();
        assert!(
            text.contains("No radar") && text.contains("timed out"),
            "{text}"
        );
        assert!(text.contains("[r] tries again"), "{text}");
    }

    /// Every size from the app's floor to a very large terminal, a pane
    /// with no interior, and a loop with no frames at all.
    #[test]
    fn awkward_sizes_and_an_empty_loop_draw_without_panicking() {
        let soaked = frame_at(0, Level::Heavy, |_, _| true);
        let app = app_showing(copenhagen(), vec![soaked]);
        for (width, height) in [
            (34, 12),
            (34, 3),
            (2, 2),
            (1, 1),
            (200, 12),
            (40, 90),
            (300, 100),
        ] {
            let buffer = drawn(&app, width, height);
            assert_eq!(buffer.area.width, width);
        }

        let empty = app_showing(copenhagen(), Vec::new());
        let buffer = drawn(&empty, 60, 15);
        assert!(braille_in(&buffer, palette().border).is_empty());
        assert!(row(&buffer, 14).contains(CREDIT));
    }

    /// On a wide pane the frame's full width is used and a band of it is
    /// shown; on a tall one, the reverse. Either way a dot is as many
    /// pixels wide as it is tall.
    #[test]
    fn the_frame_covers_the_pane_at_one_scale() {
        let wide = Dots::of(Rect::new(0, 0, 100, 20));
        assert_eq!(wide.pixel(0.0, 0.0).0, 0.0);
        assert_eq!(wide.pixel(200.0, 0.0).0, FRAME_SIDE as f64);
        let (_, top) = wide.pixel(0.0, 0.0);
        assert!(top > 0.0, "the band starts inside the frame");

        let tall = Dots::of(Rect::new(0, 0, 20, 40));
        assert_eq!(tall.pixel(0.0, 0.0).1, 0.0);
        assert_eq!(tall.pixel(0.0, 160.0).1, FRAME_SIDE as f64);

        let (x, y) = wide.dot(wide.pixel(31.0, 7.0));
        assert!((x - 31.0).abs() < 1e-9 && (y - 7.0).abs() < 1e-9);
    }
}

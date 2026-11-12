//! What the radar screen holds between frames: the pictures with their
//! map, and where the loop has got to.

use crate::app::ActiveLocation;
use crate::outline;
use crate::weather::radar::{FRAME_SIDE, Level, Radar};
use std::time::{Duration, Instant};

/// Everything a draw needs, worked out once on the worker's thread so a
/// frame of the loop costs the terminal nothing but the drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct RadarScene {
    pub location: ActiveLocation,
    pub radar: Radar,
    /// Coastlines, borders and lakes, as lines of frame pixels.
    pub outline: Vec<outline::Trace>,
    /// Whether any frame shows more than a trace. An empty map is what a
    /// dry afternoon looks like and also what a broken screen looks like,
    /// so the pane says which.
    pub has_rain: bool,
}

impl RadarScene {
    pub fn new(location: ActiveLocation, radar: Radar) -> Self {
        let outline = outline::traced_on(&radar);
        let has_rain = radar.frames.iter().any(|frame| {
            (0..FRAME_SIDE).any(|y| (0..FRAME_SIDE).any(|x| frame.level_at(x, y) > Level::Trace))
        });
        Self {
            location,
            radar,
            outline,
            has_rain,
        }
    }
}

/// How long each frame of the loop is held.
const STEP: Duration = Duration::from_millis(450);
/// The newest frame is the one that matters, and a loop that snaps from it
/// straight back to two hours ago never lets it be read.
const LAST_DWELL: Duration = Duration::from_millis(1800);

/// Which frame is on screen and whether the loop is running.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playback {
    frame: usize,
    is_playing: bool,
    shown_at: Option<Instant>,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            frame: 0,
            is_playing: true,
            shown_at: None,
        }
    }
}

impl Playback {
    /// The frame to draw, of `count`. Clamped here and not trusted from
    /// the last scene: a refresh can come back with fewer frames.
    pub fn frame(&self, count: usize) -> usize {
        self.frame.min(count.saturating_sub(1))
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    /// Move the loop on if the frame on screen has had its time. `true`
    /// when the frame changed, which is the caller's reason to redraw. The
    /// clock is the caller's, so a test steps it and nothing sleeps.
    pub fn advance(&mut self, now: Instant, count: usize) -> bool {
        if !self.is_playing || count < 2 {
            return false;
        }
        let Some(shown_at) = self.shown_at else {
            self.shown_at = Some(now);
            return false;
        };
        let frame = self.frame(count);
        let dwell = if frame + 1 == count { LAST_DWELL } else { STEP };
        if now.saturating_duration_since(shown_at) < dwell {
            return false;
        }
        self.frame = (frame + 1) % count;
        self.shown_at = Some(now);
        true
    }

    /// Stepping by hand stops the loop, or it would take the frame away
    /// again before it could be read. The ends are ends: a step past the
    /// newest frame stays on it.
    pub fn step(&mut self, forward: bool, count: usize) {
        let frame = self.frame(count);
        self.is_playing = false;
        self.frame = if forward {
            (frame + 1).min(count.saturating_sub(1))
        } else {
            frame.saturating_sub(1)
        };
    }

    pub fn toggle(&mut self) {
        self.is_playing = !self.is_playing;
        self.shown_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::radar::Frame;

    #[test]
    fn the_loop_holds_each_frame_and_lingers_on_the_newest() {
        let mut playback = Playback::default();
        let start = Instant::now();

        assert!(
            !playback.advance(start, 3),
            "the first look starts the clock"
        );
        assert!(!playback.advance(start + STEP / 2, 3));
        assert!(playback.advance(start + STEP, 3));
        assert_eq!(playback.frame(3), 1);
        assert!(playback.advance(start + STEP * 2, 3));
        assert_eq!(playback.frame(3), 2);

        assert!(!playback.advance(start + STEP * 3, 3), "the newest lingers");
        assert!(playback.advance(start + STEP * 2 + LAST_DWELL, 3));
        assert_eq!(playback.frame(3), 0, "and the loop comes round");
    }

    #[test]
    fn stepping_pauses_and_stops_at_either_end() {
        let mut playback = Playback::default();
        playback.step(false, 3);
        assert_eq!(playback.frame(3), 0);
        assert!(!playback.is_playing());

        for _ in 0..5 {
            playback.step(true, 3);
        }
        assert_eq!(playback.frame(3), 2);

        let now = Instant::now();
        assert!(!playback.advance(now, 3));
        assert!(
            !playback.advance(now + LAST_DWELL * 2, 3),
            "paused stays put"
        );

        playback.toggle();
        assert!(playback.is_playing());
        assert!(
            !playback.advance(now + LAST_DWELL * 2, 3),
            "the clock restarts"
        );
        assert!(playback.advance(now + LAST_DWELL * 3, 3));
    }

    /// One frame has nowhere to go, none has nothing to show, and a frame
    /// number left over from a longer loop is brought back in range.
    #[test]
    fn short_and_shrunken_loops_stay_in_range() {
        let now = Instant::now();
        for count in [0, 1] {
            let mut playback = Playback::default();
            assert!(!playback.advance(now, count));
            assert!(!playback.advance(now + LAST_DWELL, count));
            playback.step(true, count);
            assert_eq!(playback.frame(count), 0);
        }

        let mut playback = Playback::default();
        for _ in 0..12 {
            playback.step(true, 13);
        }
        assert_eq!(playback.frame(13), 12);
        assert_eq!(playback.frame(4), 3);
    }

    fn place() -> ActiveLocation {
        ActiveLocation {
            label: "Copenhagen".to_string(),
            lat: 55.68,
            lon: 12.57,
        }
    }

    #[test]
    fn a_scene_knows_its_outline_and_whether_anything_is_falling() {
        let trace = Frame::fixture(0, Level::Trace, |x, y| x == y);
        let rain = Frame::fixture(600, Level::Light, |x, y| x == 3 && y == 200);

        let hazy = RadarScene::new(place(), Radar::fixture(55.7, 12.6, vec![trace.clone()]));
        assert!(!hazy.has_rain, "a trace is not rain");
        assert!(!hazy.outline.is_empty());

        let wet = RadarScene::new(place(), Radar::fixture(55.7, 12.6, vec![trace, rain]));
        assert!(wet.has_rain);
    }
}

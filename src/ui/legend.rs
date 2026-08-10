use crate::app::{App, Fetch, Screen};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub(super) fn keybind_legend_render(frame: &mut Frame, app: &App, area: Rect) {
    let binds: &[(&str, &str)] = match app.screen {
        Screen::Weather => &[
            ("q", "quit"),
            ("←→", "day"),
            ("n", "now"),
            ("p", "precip"),
            ("r", "refresh"),
            ("u", "units"),
            ("l", "location"),
        ],
        // Quit and back lead, as they do on the weather screen: a narrow
        // terminal clips the tail, and those are the two worth keeping.
        Screen::Precipitation => &[
            ("q", "quit"),
            ("b", "back"),
            ("←→", "hour"),
            ("↑↓", "day"),
            ("n", "now"),
            ("r", "refresh"),
            ("u", "units"),
        ],
        Screen::Search => match &app.results {
            Fetch::Ready(l) if !l.is_empty() => {
                &[("↑↓", "navigate"), ("enter", "select"), ("esc", "cancel")]
            }
            _ => &[("enter", "search"), ("esc", "cancel")],
        },
    };

    let mut spans = vec![Span::from("  ")];
    for (key, label) in binds {
        spans.push(Span::from(format!("[{key}]")).yellow());
        spans.push(Span::from(format!(" {label}   ")).dark_gray());
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::model::Weather;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn legend_at(width: u16, screen: Screen) -> String {
        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));
        app.screen = screen;

        let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
        terminal
            .draw(|f| keybind_legend_render(f, &app, f.area()))
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        (0..width).map(|x| buffer[(x, 0)].symbol()).collect()
    }

    /// Nothing else on the weather screen mentions that `p` exists, so the
    /// legend is the only thing making the screen discoverable at all.
    #[test]
    fn the_weather_legend_advertises_the_precipitation_screen() {
        let legend = legend_at(120, Screen::Weather);
        assert!(legend.contains("[p]"), "{legend:?}");
        assert!(legend.contains("precip"), "{legend:?}");
    }

    /// The precipitation screen rebinds the arrows and takes `b` for back, so
    /// its legend must not still describe the weather screen's bindings.
    #[test]
    fn the_precipitation_legend_describes_its_own_keys() {
        let legend = legend_at(120, Screen::Precipitation);

        assert!(legend.contains("[b] back"), "{legend:?}");
        assert!(legend.contains("hour"), "{legend:?}");
        assert!(!legend.contains("[p]"), "already there: {legend:?}");
        assert!(
            !legend.contains("location"),
            "l returns to search: {legend:?}"
        );
    }

    /// A narrow terminal clips the tail of the bar, so the two keys that get
    /// you out have to come first.
    #[test]
    fn quitting_and_leaving_survive_a_narrow_terminal() {
        let legend = legend_at(24, Screen::Precipitation);
        assert!(legend.contains("[q]"), "{legend:?}");
        assert!(legend.contains("[b]"), "{legend:?}");
    }
}

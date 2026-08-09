use crate::app::App;
use crate::ui::UNKNOWN;
use crate::weather::code::description;
use crate::weather::model::Weather;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};

pub(super) fn top_area_render(frame: &mut Frame, app: &App, weather: &Weather, area: Rect) {
    let block = Block::bordered();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let name = app.location.as_deref().unwrap_or(&weather.location);

    let lines = vec![
        Line::from(name.to_uppercase()).bold().blue(),
        Line::from(weather.current.code.map_or(UNKNOWN, description)).dark_gray(),
    ];

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

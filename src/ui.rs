use std::sync::{Arc, Mutex};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::map::{Cell, Map, ResourceKind};

pub struct UiState {
    pub energy: u32,
    pub crystals: u32,
    pub robot_positions: Vec<(usize, usize, bool)>, // (x, y, is_collector)
}

/// Écrit par le thread Base (agrégation), lu par le thread de rendu.
pub type SharedUi = Arc<Mutex<UiState>>;

impl Default for UiState {
    fn default() -> Self {
        Self { energy: 0, crystals: 0, robot_positions: vec![] }
    }
}

pub fn render(frame: &mut Frame, map: &Map, state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(22)])
        .split(frame.area());

    render_map(frame, map, state, chunks[0]);
    render_panel(frame, state, chunks[1]);
}

fn render_map(frame: &mut Frame, map: &Map, state: &UiState, area: ratatui::layout::Rect) {
    let block = Block::default().title(" Map ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::with_capacity(map.height);

    for (y, row) in map.cells.iter().enumerate() {
        let mut spans: Vec<Span> = Vec::with_capacity(map.width);
        for (x, cell) in row.iter().enumerate() {
            // Robot overlay
            let robot = state.robot_positions.iter().find(|(rx, ry, _)| *rx == x && *ry == y);
            if let Some((_, _, is_collector)) = robot {
                let (ch, color) = if *is_collector {
                    ('o', Color::Magenta)
                } else {
                    ('x', Color::Red)
                };
                spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
                continue;
            }

            let (ch, color) = match cell {
                Cell::Empty => ('.', Color::DarkGray),
                Cell::Obstacle => ('O', Color::Gray),
                Cell::Resource(r) => match r.kind {
                    ResourceKind::Energy => ('E', Color::Yellow),
                    ResourceKind::Crystal => ('C', Color::Cyan),
                },
                Cell::Base => ('#', Color::Green),
            };
            spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_panel(frame: &mut Frame, state: &UiState, area: ratatui::layout::Rect) {
    let block = Block::default().title(" Stats ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = vec![
        Line::from(Span::styled("Resources", Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled(
            format!("Energy:   {}", state.energy),
            Style::default().fg(Color::Yellow),
        )),
        Line::from(Span::styled(
            format!("Crystals: {}", state.crystals),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(Span::styled("Legend", Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled(" # Base", Style::default().fg(Color::Green))),
        Line::from(Span::styled(" E Energy", Style::default().fg(Color::Yellow))),
        Line::from(Span::styled(" C Crystal", Style::default().fg(Color::Cyan))),
        Line::from(Span::styled(" O Obstacle", Style::default().fg(Color::Gray))),
        Line::from(Span::styled(" x Scout", Style::default().fg(Color::Red))),
        Line::from(Span::styled(" o Collector", Style::default().fg(Color::Magenta))),
        Line::from(""),
        Line::from(Span::styled("'q' to quit", Style::default().fg(Color::DarkGray))),
    ];

    frame.render_widget(Paragraph::new(text), inner);
}

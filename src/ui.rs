use std::collections::HashMap;
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
    pub simulation_finished: bool,
    pub robot_positions: Vec<(usize, usize, bool)>, // (x, y, is_collector)
}

pub type SharedUi = Arc<Mutex<UiState>>;

impl Default for UiState {
    fn default() -> Self {
        Self {
            energy: 0,
            crystals: 0,
            simulation_finished: false,
            robot_positions: vec![],
        }
    }
}

pub fn render(
    frame: &mut Frame,
    map: &Map,
    known_map: &HashMap<(usize, usize), Cell>,
    state: &UiState,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(24)])
        .split(frame.area());

    render_map(frame, map, known_map, state, chunks[0]);
    render_panel(frame, state, chunks[1]);
}

fn render_map(
    frame: &mut Frame,
    map: &Map,
    known_map: &HashMap<(usize, usize), Cell>,
    state: &UiState,
    area: ratatui::layout::Rect,
) {
    let block = Block::default().title(" Map ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::with_capacity(map.height);
    for y in 0..map.height {
        let mut spans = Vec::with_capacity(map.width);
        for x in 0..map.width {
            let robot = state
                .robot_positions
                .iter()
                .find(|(rx, ry, _)| *rx == x && *ry == y);

            if let Some((_, _, is_collector)) = robot {
                let (ch, color) = if *is_collector {
                    ('o', Color::Magenta)
                } else {
                    ('x', Color::Red)
                };
                spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
                continue;
            }

            let (ch, color) = cell_symbol((x, y), map.base_pos, known_map.get(&(x, y)));

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

    let status = if state.simulation_finished {
        Line::from(Span::styled(
            "Simulation terminee",
            Style::default().fg(Color::LightGreen),
        ))
    } else {
        Line::from(Span::styled(
            "Simulation active",
            Style::default().fg(Color::White),
        ))
    };

    let text = vec![
        Line::from(Span::styled("Resources", Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled(
            format!("Energy:   {}", state.energy),
            Style::default().fg(Color::Green),
        )),
        Line::from(Span::styled(
            format!("Crystals: {}", state.crystals),
            Style::default().fg(Color::LightMagenta),
        )),
        Line::from(""),
        status,
        Line::from(""),
        Line::from(Span::styled("Legend", Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled(
            " # Base",
            Style::default().fg(Color::LightGreen),
        )),
        Line::from(Span::styled(" E Energy", Style::default().fg(Color::Green))),
        Line::from(Span::styled(
            " C Crystal",
            Style::default().fg(Color::LightMagenta),
        )),
        Line::from(Span::styled(
            " O Obstacle",
            Style::default().fg(Color::LightCyan),
        )),
        Line::from(Span::styled(" x Scout", Style::default().fg(Color::Red))),
        Line::from(Span::styled(
            " o Collector",
            Style::default().fg(Color::Magenta),
        )),
        Line::from(Span::styled(
            " ? Unknown",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Any key to quit",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Paragraph::new(text), inner);
}

fn cell_symbol(
    pos: (usize, usize),
    base_pos: (usize, usize),
    known_cell: Option<&Cell>,
) -> (char, Color) {
    if pos == base_pos {
        return ('#', Color::LightGreen);
    }

    match known_cell {
        Some(Cell::Empty) => ('.', Color::DarkGray),
        Some(Cell::Obstacle) => ('O', Color::LightCyan),
        Some(Cell::Resource(resource)) => match resource.kind {
            ResourceKind::Energy => ('E', Color::Green),
            ResourceKind::Crystal => ('C', Color::LightMagenta),
        },
        Some(Cell::Base) => ('#', Color::LightGreen),
        None => ('?', Color::DarkGray),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Resource, ResourceKind};

    #[test]
    fn unknown_cell_uses_question_mark() {
        assert_eq!(cell_symbol((0, 0), (1, 1), None), ('?', Color::DarkGray));
    }

    #[test]
    fn base_is_visible_even_if_unknown() {
        assert_eq!(cell_symbol((1, 1), (1, 1), None), ('#', Color::LightGreen));
    }

    #[test]
    fn known_crystal_uses_required_symbol_and_color() {
        let cell = Cell::Resource(Resource {
            kind: ResourceKind::Crystal,
            quantity: 80,
        });

        assert_eq!(
            cell_symbol((2, 2), (1, 1), Some(&cell)),
            ('C', Color::LightMagenta)
        );
    }
}

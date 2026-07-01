#![allow(dead_code)]

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};

mod base;
mod map;
mod messages;
mod robot;
mod ui;

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let map = map::Map::new(60, 25);
    let ui_state = ui::UiState::default();

    let result = run(&mut terminal, map, ui_state);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    map: map::Map,
    mut ui_state: ui::UiState,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, &map, &ui_state))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    _ => {}
                }
            }
        }

        // Sprint 3: robot threads will update ui_state via Arc<Mutex<UiState>>
        let _ = &mut ui_state;
    }
    Ok(())
}

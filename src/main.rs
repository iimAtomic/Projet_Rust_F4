use std::collections::HashMap;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use crossbeam_channel::unbounded;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

mod base;
mod map;
mod messages;
mod robot;
mod ui;

use base::Base;
use map::Map;
use robot::{Collector, Scout};
use ui::UiState;

const MAP_WIDTH: usize = 60;
const MAP_HEIGHT: usize = 25;
const SCOUT_COUNT: usize = 2;
const COLLECTOR_COUNT: usize = 2;
const COLLECTOR_CAPACITY: u32 = 5;
const ROBOT_TICK: Duration = Duration::from_millis(150);

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = catch_unwind(AssertUnwindSafe(|| run(&mut terminal)));

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    match result {
        Ok(inner) => inner,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let shared_map = Map::new_shared(MAP_WIDTH, MAP_HEIGHT);
    let base_pos = shared_map
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .base_pos;

    let known_map: base::SharedKnownMap = Arc::new(RwLock::new(HashMap::new()));
    let shared_ui: ui::SharedUi = Arc::new(Mutex::new(UiState::default()));
    let stop = Arc::new(AtomicBool::new(false));

    let (tx, rx) = unbounded::<messages::RobotMessage>();

    let base = Base::new(rx, known_map.clone(), shared_ui.clone());
    let base_stop = stop.clone();
    let base_handle = thread::spawn(move || base.run(base_stop));

    let mut robot_handles = Vec::new();

    for id in 0..SCOUT_COUNT {
        let mut scout = Scout::new(id, base_pos, tx.clone());
        let map = shared_map.clone();
        let stop = stop.clone();
        robot_handles.push(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                scout.step(&map);
                thread::sleep(ROBOT_TICK);
            }
        }));
    }

    for id in 0..COLLECTOR_COUNT {
        let mut collector = Collector::new(id, base_pos, COLLECTOR_CAPACITY, tx.clone());
        let map = shared_map.clone();
        let known_map = known_map.clone();
        let stop = stop.clone();
        robot_handles.push(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                collector.step(&map, &known_map);
                thread::sleep(ROBOT_TICK);
            }
        }));
    }

    drop(tx);

    let render_result = render_loop(terminal, &shared_map, &known_map, &shared_ui, &stop);

    stop.store(true, Ordering::Relaxed);
    for handle in robot_handles {
        let _ = handle.join();
    }
    let _ = base_handle.join();

    render_result
}

fn render_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    shared_map: &map::SharedMap,
    known_map: &base::SharedKnownMap,
    shared_ui: &ui::SharedUi,
    stop: &Arc<AtomicBool>,
) -> io::Result<()> {
    let mut simulation_finished = false;

    loop {
        let has_resources = {
            let map = shared_map.read().unwrap_or_else(|e| e.into_inner());
            map.has_resources()
        };

        if !has_resources {
            simulation_finished = true;
            stop.store(true, Ordering::Relaxed);
            let mut ui_state = shared_ui.lock().unwrap_or_else(|e| e.into_inner());
            ui_state.simulation_finished = true;
        }

        {
            let map = shared_map.read().unwrap_or_else(|e| e.into_inner());
            let known_snapshot = known_map.read().unwrap_or_else(|e| e.into_inner()).clone();
            let ui_state = shared_ui.lock().unwrap_or_else(|e| e.into_inner());
            terminal.draw(|frame| ui::render(frame, &map, &known_snapshot, &ui_state))?;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(_) = event::read()? {
                break;
            }
        }

        if stop.load(Ordering::Relaxed) && !simulation_finished {
            break;
        }
    }

    Ok(())
}

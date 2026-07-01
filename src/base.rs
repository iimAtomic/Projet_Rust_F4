use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError};

use crate::map::{Cell, ResourceKind};
use crate::messages::{RobotKind, RobotMessage};
use crate::ui::SharedUi;

pub type SharedKnownMap = Arc<RwLock<HashMap<(usize, usize), Cell>>>;

pub struct Base {
    pub pos: (usize, usize),
    pub energy_collected: u32,
    pub crystals_collected: u32,
    pub known_map: SharedKnownMap,
    ui: SharedUi,
    robot_positions: HashMap<usize, (RobotKind, (usize, usize))>,
    receiver: Receiver<RobotMessage>,
}

impl Base {
    pub fn new(
        pos: (usize, usize),
        receiver: Receiver<RobotMessage>,
        known_map: SharedKnownMap,
        ui: SharedUi,
    ) -> Self {
        Self {
            pos,
            energy_collected: 0,
            crystals_collected: 0,
            known_map,
            ui,
            robot_positions: HashMap::new(),
            receiver,
        }
    }

    pub fn process_messages(&mut self) {
        while let Ok(msg) = self.receiver.try_recv() {
            self.handle(msg);
        }
    }

    fn handle(&mut self, msg: RobotMessage) {
        match msg {
            RobotMessage::ResourceFound { pos, kind, quantity } => {
                self.with_known_map(|map| {
                    map.insert(pos, Cell::Resource(crate::map::Resource { kind, quantity }));
                });
            }
            RobotMessage::ObstacleFound { pos } => {
                self.with_known_map(|map| {
                    map.insert(pos, Cell::Obstacle);
                });
            }
            RobotMessage::ResourceCollected { pos, kind, amount } => {
                match kind {
                    ResourceKind::Energy => self.energy_collected += amount,
                    ResourceKind::Crystal => self.crystals_collected += amount,
                }

                self.with_known_map(|map| {
                    if let Some(Cell::Resource(r)) = map.get_mut(&pos) {
                        r.quantity = r.quantity.saturating_sub(amount);
                        if r.quantity == 0 {
                            map.remove(&pos);
                        }
                    }
                });
            }
            RobotMessage::RobotMoved { robot_id, kind, pos } => {
                self.robot_positions.insert(robot_id, (kind, pos));
            }
        }
    }

    fn with_known_map(&self, f: impl FnOnce(&mut HashMap<(usize, usize), Cell>)) {
        let mut map = self.known_map.write().unwrap_or_else(|e| e.into_inner());
        f(&mut map);
    }

    fn sync_ui(&self) {
        let mut ui = self.ui.lock().unwrap_or_else(|e| e.into_inner());
        ui.energy = self.energy_collected;
        ui.crystals = self.crystals_collected;
        ui.robot_positions = self
            .robot_positions
            .values()
            .map(|(kind, (x, y))| (*x, *y, *kind == RobotKind::Collector))
            .collect();
    }

    pub fn run(mut self, stop: Arc<AtomicBool>) {
        loop {
            match self.receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(msg) => {
                    self.handle(msg);
                    self.process_messages(); // vide le reste du batch sans bloquer
                    self.sync_ui();
                }
                Err(RecvTimeoutError::Timeout) => {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::UiState;
    use crossbeam_channel::unbounded;
    use std::sync::Mutex;

    fn base_with(receiver: Receiver<RobotMessage>) -> Base {
        let known_map = Arc::new(RwLock::new(HashMap::new()));
        let ui = Arc::new(Mutex::new(UiState::default()));
        Base::new((0, 0), receiver, known_map, ui)
    }

    #[test]
    fn aggregates_resource_discovery_into_known_map() {
        let (tx, rx) = unbounded();
        let mut base = base_with(rx);
        tx.send(RobotMessage::ResourceFound {
            pos: (2, 3),
            kind: ResourceKind::Crystal,
            quantity: 5,
        })
        .unwrap();

        base.process_messages();

        let known = base.known_map.read().unwrap();
        assert!(matches!(known.get(&(2, 3)), Some(Cell::Resource(r)) if r.quantity == 5));
    }

    #[test]
    fn resource_collected_updates_counters_and_depletes_known_map() {
        let (tx, rx) = unbounded();
        let mut base = base_with(rx);
        tx.send(RobotMessage::ResourceFound {
            pos: (1, 1),
            kind: ResourceKind::Energy,
            quantity: 2,
        })
        .unwrap();
        tx.send(RobotMessage::ResourceCollected {
            pos: (1, 1),
            kind: ResourceKind::Energy,
            amount: 2,
        })
        .unwrap();

        base.process_messages();

        assert_eq!(base.energy_collected, 2);
        assert_eq!(base.crystals_collected, 0);
        assert!(base.known_map.read().unwrap().get(&(1, 1)).is_none());
    }

    #[test]
    fn robot_moved_updates_ui_positions_after_sync() {
        let (tx, rx) = unbounded();
        let mut base = base_with(rx);
        tx.send(RobotMessage::RobotMoved { robot_id: 7, kind: RobotKind::Collector, pos: (4, 4) })
            .unwrap();

        base.process_messages();
        base.sync_ui();

        let ui = base.ui.lock().unwrap();
        assert_eq!(ui.robot_positions, vec![(4, 4, true)]);
    }
}

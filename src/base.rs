use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError};

use crate::map::{Cell, Resource, ResourceKind};
use crate::messages::{RobotKind, RobotMessage};
use crate::ui::SharedUi;

pub type SharedKnownMap = Arc<RwLock<HashMap<(usize, usize), Cell>>>;

pub struct Base {
    pub energy_collected: u32,
    pub crystals_collected: u32,
    pub known_map: SharedKnownMap,
    ui: SharedUi,
    robot_positions: HashMap<usize, (RobotKind, (usize, usize))>,
    receiver: Receiver<RobotMessage>,
}

impl Base {
    pub fn new(receiver: Receiver<RobotMessage>, known_map: SharedKnownMap, ui: SharedUi) -> Self {
        Self {
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
        self.sync_ui();
    }

    fn handle(&mut self, msg: RobotMessage) {
        match msg {
            RobotMessage::ResourceFound {
                pos,
                kind,
                quantity,
            } => {
                self.with_known_map(|map| {
                    map.insert(pos, Cell::Resource(Resource { kind, quantity }));
                });
            }
            RobotMessage::ObstacleFound { pos } => {
                self.with_known_map(|map| {
                    map.insert(pos, Cell::Obstacle);
                });
            }
            RobotMessage::CellSeen { pos, cell } => {
                self.with_known_map(|map| {
                    map.insert(pos, cell);
                });
            }
            RobotMessage::ResourceCollected { pos, kind, amount } => {
                match kind {
                    ResourceKind::Energy => self.energy_collected += amount,
                    ResourceKind::Crystal => self.crystals_collected += amount,
                }

                self.with_known_map(|map| {
                    if let Some(Cell::Resource(resource)) = map.get_mut(&pos) {
                        resource.quantity = resource.quantity.saturating_sub(amount);
                        if resource.quantity == 0 {
                            map.remove(&pos);
                        }
                    }
                });
            }
            RobotMessage::RobotMoved {
                robot_id,
                kind,
                pos,
            } => {
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
                    self.process_messages();
                }
                Err(RecvTimeoutError::Timeout) => {
                    if stop.load(Ordering::Relaxed) {
                        self.process_messages();
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.process_messages();
                    break;
                }
            }
        }
    }
}

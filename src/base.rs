use std::collections::HashMap;

use crossbeam_channel::Receiver;

use crate::map::{Cell, ResourceKind};
use crate::messages::RobotMessage;

pub struct Base {
    pub pos: (usize, usize),
    pub energy_collected: u32,
    pub crystals_collected: u32,
    /// Aggregated map knowledge from scouts
    pub known_map: HashMap<(usize, usize), Cell>,
    receiver: Receiver<RobotMessage>,
}

impl Base {
    pub fn new(pos: (usize, usize), receiver: Receiver<RobotMessage>) -> Self {
        Self {
            pos,
            energy_collected: 0,
            crystals_collected: 0,
            known_map: HashMap::new(),
            receiver,
        }
    }

    /// Drain all pending messages and update global state
    pub fn process_messages(&mut self) {
        while let Ok(msg) = self.receiver.try_recv() {
            match msg {
                RobotMessage::ResourceFound { pos, kind, quantity } => {
                    self.known_map.insert(
                        pos,
                        Cell::Resource(crate::map::Resource { kind, quantity }),
                    );
                }
                RobotMessage::ObstacleFound { pos } => {
                    self.known_map.insert(pos, Cell::Obstacle);
                }
                RobotMessage::ResourceCollected { kind, amount, .. } => match kind {
                    ResourceKind::Energy => self.energy_collected += amount,
                    ResourceKind::Crystal => self.crystals_collected += amount,
                },
                RobotMessage::RobotMoved { .. } => {}
            }
        }
    }
}

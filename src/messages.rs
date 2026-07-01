use crate::map::ResourceKind;

#[derive(Debug, Clone)]
pub enum RobotMessage {
    ResourceFound {
        pos: (usize, usize),
        kind: ResourceKind,
        quantity: u32,
    },
    ObstacleFound {
        pos: (usize, usize),
    },
    ResourceCollected {
        pos: (usize, usize),
        kind: ResourceKind,
        amount: u32,
    },
    RobotMoved {
        robot_id: usize,
        pos: (usize, usize),
    },
}

use std::collections::{HashMap, HashSet, VecDeque};

use crossbeam_channel::Sender;
use rand::seq::SliceRandom;

use crate::base::SharedKnownMap;
use crate::map::{Cell, SharedMap};
use crate::messages::{RobotKind, RobotMessage};

#[derive(Debug, Clone, PartialEq)]
pub enum CollectorState {
    Idle,
    MovingToResource(Vec<(usize, usize)>),
    Collecting { pos: (usize, usize) },
    ReturningToBase(Vec<(usize, usize)>),
}

pub struct Collector {
    pub id: usize,
    pub pos: (usize, usize),
    pub cargo: u32,
    pub cargo_capacity: u32,
    pub base_pos: (usize, usize),
    pub state: CollectorState,
    sender: Sender<RobotMessage>,
}

impl Collector {
    pub fn new(
        id: usize,
        base_pos: (usize, usize),
        cargo_capacity: u32,
        sender: Sender<RobotMessage>,
    ) -> Self {
        Self {
            id,
            pos: base_pos,
            cargo: 0,
            cargo_capacity,
            base_pos,
            state: CollectorState::Idle,
            sender,
        }
    }

    fn select_target(&self, known_map: &HashMap<(usize, usize), Cell>) -> Option<(usize, usize)> {
        let (cx, cy) = self.pos;
        known_map
            .iter()
            .filter(|(_, cell)| matches!(cell, Cell::Resource(r) if r.quantity > 0))
            .min_by_key(|((x, y), _)| {
                ((*x as isize - cx as isize).abs() + (*y as isize - cy as isize).abs()) as u32
            })
            .map(|(pos, _)| *pos)
    }

    pub fn bfs_path(
        &self,
        start: (usize, usize),
        goal: (usize, usize),
        known_map: &HashMap<(usize, usize), Cell>,
        map_width: usize,
        map_height: usize,
    ) -> Option<Vec<(usize, usize)>> {
        if start == goal {
            return Some(vec![]);
        }

        let mut visited = vec![vec![false; map_width]; map_height];
        let mut parent: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
        let mut queue = VecDeque::new();

        visited[start.1][start.0] = true;
        queue.push_back(start);

        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in neighbors(x, y, map_width, map_height) {
                if visited[ny][nx] {
                    continue;
                }
                if matches!(known_map.get(&(nx, ny)), Some(Cell::Obstacle)) {
                    continue;
                }
                visited[ny][nx] = true;
                parent.insert((nx, ny), (x, y));
                if (nx, ny) == goal {
                    return Some(reconstruct_path(&parent, start, goal));
                }
                queue.push_back((nx, ny));
            }
        }
        None
    }

    pub fn step(&mut self, map: &SharedMap, known_map: &SharedKnownMap) {
        let (map_width, map_height) = {
            let m = map.read().unwrap_or_else(|e| e.into_inner());
            (m.width, m.height)
        };
        let known_snapshot = known_map.read().unwrap_or_else(|e| e.into_inner()).clone();

        let state = std::mem::replace(&mut self.state, CollectorState::Idle);

        self.state = match state {
            CollectorState::Idle => {
                if self.cargo >= self.cargo_capacity {
                    self.go_home(&known_snapshot, map_width, map_height)
                } else if let Some(target) = self.select_target(&known_snapshot) {
                    match self.bfs_path(self.pos, target, &known_snapshot, map_width, map_height) {
                        Some(path) => CollectorState::MovingToResource(path),
                        None => CollectorState::Idle,
                    }
                } else {
                    CollectorState::Idle
                }
            }

            CollectorState::MovingToResource(mut path) => {
                if path.is_empty() {
                    let m = map.read().unwrap_or_else(|e| e.into_inner());
                    match m.get(self.pos.0, self.pos.1) {
                        Cell::Resource(_) => CollectorState::Collecting { pos: self.pos },
                        _ => CollectorState::Idle, // resource gone, reselect next tick
                    }
                } else {
                    self.pos = path.remove(0);
                    self.notify_moved();
                    CollectorState::MovingToResource(path)
                }
            }

            CollectorState::Collecting { pos } => {
                if self.cargo >= self.cargo_capacity {
                    self.go_home(&known_snapshot, map_width, map_height)
                } else {
                    let mut m = map.write().unwrap_or_else(|e| e.into_inner());
                    match m.try_collect(pos, 1) {
                        Some((kind, taken, remaining)) => {
                            drop(m);
                            self.cargo += taken;
                            let _ = self.sender.send(RobotMessage::ResourceCollected {
                                pos,
                                kind,
                                amount: taken,
                            });
                            if remaining == 0 {
                                self.go_home(&known_snapshot, map_width, map_height)
                            } else {
                                CollectorState::Collecting { pos }
                            }
                        }
                        None => CollectorState::Idle, // resource exhausted by another robot
                    }
                }
            }

            CollectorState::ReturningToBase(mut path) => {
                if path.is_empty() {
                    self.cargo = 0;
                    CollectorState::Idle
                } else {
                    self.pos = path.remove(0);
                    self.notify_moved();
                    CollectorState::ReturningToBase(path)
                }
            }
        };
    }

    fn go_home(
        &self,
        known_map: &HashMap<(usize, usize), Cell>,
        map_width: usize,
        map_height: usize,
    ) -> CollectorState {
        match self.bfs_path(self.pos, self.base_pos, known_map, map_width, map_height) {
            Some(path) => CollectorState::ReturningToBase(path),
            None => CollectorState::Idle,
        }
    }

    fn notify_moved(&self) {
        let _ = self.sender.send(RobotMessage::RobotMoved {
            robot_id: self.id,
            kind: RobotKind::Collector,
            pos: self.pos,
        });
    }
}

fn neighbors(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> impl Iterator<Item = (usize, usize)> {
    let mut out = Vec::with_capacity(4);
    if x > 0 { out.push((x - 1, y)); }
    if x + 1 < width { out.push((x + 1, y)); }
    if y > 0 { out.push((x, y - 1)); }
    if y + 1 < height { out.push((x, y + 1)); }
    out.into_iter()
}

fn reconstruct_path(
    parent: &HashMap<(usize, usize), (usize, usize)>,
    start: (usize, usize),
    goal: (usize, usize),
) -> Vec<(usize, usize)> {
    let mut path = vec![];
    let mut node = goal;
    while node != start {
        path.push(node);
        node = parent[&node];
    }
    path.reverse();
    path
}

pub struct Scout {
    pub id: usize,
    pub pos: (usize, usize),
    reported: HashSet<(usize, usize)>,
    sender: Sender<RobotMessage>,
}

impl Scout {
    pub fn new(id: usize, base_pos: (usize, usize), sender: Sender<RobotMessage>) -> Self {
        Self { id, pos: base_pos, reported: HashSet::new(), sender }
    }

    pub fn step(&mut self, map: &SharedMap) {
        let m = map.read().unwrap_or_else(|e| e.into_inner());

        for (nx, ny) in neighbors(self.pos.0, self.pos.1, m.width, m.height) {
            if self.reported.contains(&(nx, ny)) {
                continue;
            }
            match m.get(nx, ny) {
                Cell::Obstacle => {
                    let _ = self.sender.send(RobotMessage::ObstacleFound { pos: (nx, ny) });
                    self.reported.insert((nx, ny));
                }
                Cell::Resource(r) => {
                    let _ = self.sender.send(RobotMessage::ResourceFound {
                        pos: (nx, ny),
                        kind: r.kind.clone(),
                        quantity: r.quantity,
                    });
                    self.reported.insert((nx, ny));
                }
                _ => {}
            }
        }

        // Se déplace vers un voisin passable choisi au hasard.
        let passable: Vec<(usize, usize)> = neighbors(self.pos.0, self.pos.1, m.width, m.height)
            .filter(|(x, y)| m.is_passable(*x, *y))
            .collect();
        drop(m);

        if let Some(&next) = passable.choose(&mut rand::thread_rng()) {
            self.pos = next;
            let _ = self.sender.send(RobotMessage::RobotMoved {
                robot_id: self.id,
                kind: RobotKind::Scout,
                pos: self.pos,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Map, Resource, ResourceKind};
    use crossbeam_channel::unbounded;
    use std::sync::{Arc, RwLock};

    fn empty_known_map() -> HashMap<(usize, usize), Cell> {
        HashMap::new()
    }

    #[test]
    fn bfs_finds_direct_path_with_no_known_obstacles() {
        let (tx, _rx) = unbounded();
        let collector = Collector::new(0, (0, 0), 10, tx);
        let path = collector
            .bfs_path((0, 0), (2, 0), &empty_known_map(), 5, 5)
            .unwrap();
        assert_eq!(path, vec![(1, 0), (2, 0)]);
    }

    #[test]
    fn bfs_routes_around_known_obstacle() {
        let (tx, _rx) = unbounded();
        let collector = Collector::new(0, (0, 0), 10, tx);
        let mut known = empty_known_map();
        known.insert((1, 0), Cell::Obstacle);

        let path = collector.bfs_path((0, 0), (2, 0), &known, 5, 5).unwrap();
        assert!(!path.contains(&(1, 0)));
        assert_eq!(*path.last().unwrap(), (2, 0));
    }

    #[test]
    fn bfs_returns_none_when_goal_is_walled_off() {
        let (tx, _rx) = unbounded();
        let collector = Collector::new(0, (0, 0), 10, tx);
        let mut known = empty_known_map();
        known.insert((0, 1), Cell::Obstacle);
        known.insert((1, 0), Cell::Obstacle);
        known.insert((2, 1), Cell::Obstacle);
        known.insert((1, 2), Cell::Obstacle);

        assert!(collector.bfs_path((0, 0), (1, 1), &known, 3, 3).is_none());
    }

    #[test]
    fn collector_full_cycle_moves_collects_and_returns() {
        let mut map = Map::new(5, 5);
        map.cells[0][1] = Cell::Resource(Resource { kind: ResourceKind::Energy, quantity: 1 });
        let base_pos = map.base_pos;
        let shared_map: SharedMap = Arc::new(RwLock::new(map));
        let known_map: SharedKnownMap = Arc::new(RwLock::new(HashMap::new()));
        known_map.write().unwrap().insert(
            (1, 0),
            Cell::Resource(Resource { kind: ResourceKind::Energy, quantity: 1 }),
        );

        let (tx, rx) = unbounded();
        let mut collector = Collector::new(0, base_pos, 5, tx);
        collector.pos = (0, 0);

        for _ in 0..20 {
            collector.step(&shared_map, &known_map);
            if collector.state == CollectorState::Idle && collector.pos == base_pos && collector.cargo == 0 {
                break;
            }
        }

        let messages: Vec<_> = rx.try_iter().collect();
        assert!(messages
            .iter()
            .any(|m| matches!(m, RobotMessage::ResourceCollected { amount: 1, .. })));
        assert!(matches!(shared_map.read().unwrap().get(1, 0), Cell::Empty));
    }

    #[test]
    fn scout_reports_neighboring_resource_and_moves() {
        let mut map = Map::new(5, 5);
        map.cells[0][1] = Cell::Resource(Resource { kind: ResourceKind::Crystal, quantity: 4 });
        let base_pos = map.base_pos;
        let shared_map: SharedMap = Arc::new(RwLock::new(map));

        let (tx, rx) = unbounded();
        let mut scout = Scout::new(0, base_pos, tx);
        scout.pos = (0, 0);

        scout.step(&shared_map);

        let messages: Vec<_> = rx.try_iter().collect();
        assert!(messages.iter().any(|m| matches!(
            m,
            RobotMessage::ResourceFound { pos: (1, 0), kind: ResourceKind::Crystal, quantity: 4 }
        )));
        assert!(messages.iter().any(|m| matches!(m, RobotMessage::RobotMoved { .. })));
    }
}

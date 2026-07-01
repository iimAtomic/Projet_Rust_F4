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
            .filter(|(_, cell)| matches!(cell, Cell::Resource(resource) if resource.quantity > 0))
            .min_by_key(|((x, y), _)| x.abs_diff(cx) + y.abs_diff(cy))
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
        let mut parent = HashMap::new();
        let mut queue = VecDeque::new();

        visited[start.1][start.0] = true;
        queue.push_back(start);

        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in neighbors(x, y, map_width, map_height) {
                if visited[ny][nx] || matches!(known_map.get(&(nx, ny)), Some(Cell::Obstacle)) {
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
            let map = map.read().unwrap_or_else(|e| e.into_inner());
            (map.width, map.height)
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
                    let map = map.read().unwrap_or_else(|e| e.into_inner());
                    match map.get(self.pos.0, self.pos.1) {
                        Cell::Resource(_) => CollectorState::Collecting { pos: self.pos },
                        _ => CollectorState::Idle,
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
                    let mut map = map.write().unwrap_or_else(|e| e.into_inner());
                    match map.try_collect(pos, 1) {
                        Some((kind, taken, remaining)) => {
                            drop(map);
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
                        None => CollectorState::Idle,
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

pub struct Scout {
    pub id: usize,
    pub pos: (usize, usize),
    reported: HashSet<(usize, usize)>,
    sender: Sender<RobotMessage>,
}

impl Scout {
    pub fn new(id: usize, base_pos: (usize, usize), sender: Sender<RobotMessage>) -> Self {
        Self {
            id,
            pos: base_pos,
            reported: HashSet::new(),
            sender,
        }
    }

    pub fn step(&mut self, map: &SharedMap) {
        let map = map.read().unwrap_or_else(|e| e.into_inner());

        for (nx, ny) in neighbors(self.pos.0, self.pos.1, map.width, map.height) {
            if self.reported.contains(&(nx, ny)) {
                continue;
            }

            match map.get(nx, ny) {
                Cell::Obstacle => {
                    let _ = self
                        .sender
                        .send(RobotMessage::ObstacleFound { pos: (nx, ny) });
                    self.reported.insert((nx, ny));
                }
                Cell::Resource(resource) => {
                    let _ = self.sender.send(RobotMessage::ResourceFound {
                        pos: (nx, ny),
                        kind: resource.kind,
                        quantity: resource.quantity,
                    });
                    self.reported.insert((nx, ny));
                }
                Cell::Empty | Cell::Base => {
                    let _ = self.sender.send(RobotMessage::CellSeen {
                        pos: (nx, ny),
                        cell: map.get(nx, ny).clone(),
                    });
                    self.reported.insert((nx, ny));
                }
            }
        }

        let passable: Vec<_> = neighbors(self.pos.0, self.pos.1, map.width, map.height)
            .filter(|(x, y)| map.is_passable(*x, *y))
            .collect();
        drop(map);

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

fn neighbors(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> impl Iterator<Item = (usize, usize)> {
    let mut out = Vec::with_capacity(4);
    if x > 0 {
        out.push((x - 1, y));
    }
    if x + 1 < width {
        out.push((x + 1, y));
    }
    if y > 0 {
        out.push((x, y - 1));
    }
    if y + 1 < height {
        out.push((x, y + 1));
    }
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

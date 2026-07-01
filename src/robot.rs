use std::collections::{HashMap, VecDeque};

use crossbeam_channel::Sender;

use crate::map::{Cell, Map};
use crate::messages::RobotMessage;

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
    /// Local knowledge: pos → Cell discovered by scouts
    pub known_map: HashMap<(usize, usize), Cell>,
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
            known_map: HashMap::new(),
            cargo: 0,
            cargo_capacity,
            base_pos,
            state: CollectorState::Idle,
            sender,
        }
    }

    /// Receive a discovery from a scout and update local known_map
    pub fn learn(&mut self, pos: (usize, usize), cell: Cell) {
        self.known_map.insert(pos, cell);
    }

    /// Manhattan-nearest known resource with quantity > 0
    fn select_target(&self) -> Option<(usize, usize)> {
        let (cx, cy) = self.pos;
        self.known_map
            .iter()
            .filter(|(_, cell)| matches!(cell, Cell::Resource(r) if r.quantity > 0))
            .min_by_key(|((x, y), _)| {
                ((*x as isize - cx as isize).abs() + (*y as isize - cy as isize).abs()) as u32
            })
            .map(|(pos, _)| *pos)
    }

    /// BFS from `start` to `goal` using local known_map (unknown = passable).
    /// Returns the path excluding `start`, or None if unreachable.
    pub fn bfs_path(
        &self,
        start: (usize, usize),
        goal: (usize, usize),
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
                // Known obstacle: skip. Unknown or anything else: passable.
                if matches!(self.known_map.get(&(nx, ny)), Some(Cell::Obstacle)) {
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

    /// Advance one simulation tick. Mutates `map` for resource collection.
    /// In the concurrent version this will use Arc<RwLock<Map>> + messages.
    pub fn step(&mut self, map: &mut Map) {
        let state = std::mem::replace(&mut self.state, CollectorState::Idle);

        self.state = match state {
            CollectorState::Idle => {
                if self.cargo >= self.cargo_capacity {
                    self.go_home(map)
                } else if let Some(target) = self.select_target() {
                    match self.bfs_path(self.pos, target, map.width, map.height) {
                        Some(path) => CollectorState::MovingToResource(path),
                        None => CollectorState::Idle,
                    }
                } else {
                    CollectorState::Idle
                }
            }

            CollectorState::MovingToResource(mut path) => {
                if path.is_empty() {
                    // Arrived — check resource still there
                    match map.get(self.pos.0, self.pos.1) {
                        Cell::Resource(_) => CollectorState::Collecting { pos: self.pos },
                        _ => CollectorState::Idle, // resource gone, reselect next tick
                    }
                } else {
                    self.pos = path.remove(0);
                    let _ = self.sender.send(RobotMessage::RobotMoved {
                        robot_id: self.id,
                        pos: self.pos,
                    });
                    CollectorState::MovingToResource(path)
                }
            }

            CollectorState::Collecting { pos } => {
                if self.cargo >= self.cargo_capacity {
                    return self.state = self.go_home(map);
                }
                match &mut map.cells[pos.1][pos.0] {
                    Cell::Resource(r) if r.quantity > 0 => {
                        r.quantity -= 1;
                        self.cargo += 1;
                        let kind = r.kind.clone();
                        let remaining = r.quantity;
                        let _ = self.sender.send(RobotMessage::ResourceCollected {
                            pos,
                            kind,
                            amount: 1,
                        });
                        if remaining == 0 {
                            map.cells[pos.1][pos.0] = Cell::Empty;
                            // also remove from local known_map
                            self.known_map.remove(&pos);
                            self.go_home(map)
                        } else {
                            CollectorState::Collecting { pos }
                        }
                    }
                    _ => CollectorState::Idle, // resource exhausted by another robot
                }
            }

            CollectorState::ReturningToBase(mut path) => {
                if path.is_empty() {
                    // Depositing at base
                    self.cargo = 0;
                    CollectorState::Idle
                } else {
                    self.pos = path.remove(0);
                    let _ = self.sender.send(RobotMessage::RobotMoved {
                        robot_id: self.id,
                        pos: self.pos,
                    });
                    CollectorState::ReturningToBase(path)
                }
            }
        };
    }

    fn go_home(&self, map: &Map) -> CollectorState {
        match self.bfs_path(self.pos, self.base_pos, map.width, map.height) {
            Some(path) => CollectorState::ReturningToBase(path),
            None => CollectorState::Idle,
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

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

// ── Scout stub (P2 will implement) ───────────────────────────────────────────

pub struct Scout {
    pub id: usize,
    pub pos: (usize, usize),
    sender: Sender<RobotMessage>,
}

impl Scout {
    pub fn new(id: usize, base_pos: (usize, usize), sender: Sender<RobotMessage>) -> Self {
        Self { id, pos: base_pos, sender }
    }

    /// P2 will implement random exploration logic
    pub fn step(&mut self, _map: &Map) {
        let _ = &self.sender;
    }
}

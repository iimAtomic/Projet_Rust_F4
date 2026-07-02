use std::sync::{Arc, RwLock};

use noise::{NoiseFn, Perlin};
use rand::Rng;

const OBSTACLE_SCALE: f64 = 0.15;
const OBSTACLE_THRESHOLD: f64 = 0.42;
const RESOURCE_DENSITY: f64 = 0.06;
const MIN_RESOURCE_QUANTITY: u32 = 50;
const MAX_RESOURCE_QUANTITY: u32 = 200;
const BASE_CLEAR_RADIUS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResourceKind {
    Energy,
    Crystal,
}

#[derive(Debug, Clone)]
pub struct Resource {
    pub kind: ResourceKind,
    pub quantity: u32,
}

#[derive(Debug, Clone)]
pub enum Cell {
    Empty,
    Obstacle,
    Resource(Resource),
    Base,
}

pub struct Map {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Vec<Cell>>,
    pub base_pos: (usize, usize),
}

pub type SharedMap = Arc<RwLock<Map>>;

impl Map {
    pub fn new(width: usize, height: usize) -> Self {
        let base_pos = (width / 2, height / 2);
        let perlin = Perlin::new(rand::thread_rng().r#gen());
        let mut rng = rand::thread_rng();

        let mut cells = vec![vec![Cell::Empty; width]; height];
        for (y, row) in cells.iter_mut().enumerate() {
            for (x, cell) in row.iter_mut().enumerate() {
                if distance_from_base((x, y), base_pos) <= BASE_CLEAR_RADIUS {
                    continue;
                }

                let n = perlin.get([x as f64 * OBSTACLE_SCALE, y as f64 * OBSTACLE_SCALE]);
                if n > OBSTACLE_THRESHOLD {
                    *cell = Cell::Obstacle;
                }
            }
        }

        let resource_count = ((width * height) as f64 * RESOURCE_DENSITY).round() as usize;
        let mut placed = 0;
        let max_attempts = resource_count * 20;

        for _ in 0..max_attempts {
            if placed >= resource_count {
                break;
            }

            let x = rng.gen_range(0..width);
            let y = rng.gen_range(0..height);
            if !matches!(cells[y][x], Cell::Empty) || (x, y) == base_pos {
                continue;
            }

            let kind = if rng.gen_bool(0.5) {
                ResourceKind::Energy
            } else {
                ResourceKind::Crystal
            };

            cells[y][x] = Cell::Resource(Resource {
                kind,
                quantity: rng.gen_range(MIN_RESOURCE_QUANTITY..=MAX_RESOURCE_QUANTITY),
            });
            placed += 1;
        }

        if placed == 0
            && width > 0
            && height > 0
            && let Some((x, y)) = find_first_empty(&cells, base_pos)
        {
            cells[y][x] = Cell::Resource(Resource {
                kind: ResourceKind::Energy,
                quantity: rng.gen_range(MIN_RESOURCE_QUANTITY..=MAX_RESOURCE_QUANTITY),
            });
        }

        cells[base_pos.1][base_pos.0] = Cell::Base;

        Self {
            width,
            height,
            cells,
            base_pos,
        }
    }

    pub fn new_shared(width: usize, height: usize) -> SharedMap {
        Arc::new(RwLock::new(Self::new(width, height)))
    }

    pub fn get(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y][x]
    }

    pub fn is_passable(&self, x: usize, y: usize) -> bool {
        !matches!(self.get(x, y), Cell::Obstacle)
    }

    pub fn has_resources(&self) -> bool {
        self.cells
            .iter()
            .flatten()
            .any(|cell| matches!(cell, Cell::Resource(_)))
    }

    pub fn try_collect(
        &mut self,
        pos: (usize, usize),
        amount: u32,
    ) -> Option<(ResourceKind, u32, u32)> {
        match &mut self.cells[pos.1][pos.0] {
            Cell::Resource(r) if r.quantity > 0 => {
                let taken = amount.min(r.quantity);
                r.quantity -= taken;
                let kind = r.kind;
                let remaining = r.quantity;
                if remaining == 0 {
                    self.cells[pos.1][pos.0] = Cell::Empty;
                }
                Some((kind, taken, remaining))
            }
            _ => None,
        }
    }
}

fn distance_from_base(pos: (usize, usize), base_pos: (usize, usize)) -> usize {
    pos.0.abs_diff(base_pos.0) + pos.1.abs_diff(base_pos.1)
}

fn find_first_empty(cells: &[Vec<Cell>], base_pos: (usize, usize)) -> Option<(usize, usize)> {
    cells.iter().enumerate().find_map(|(y, row)| {
        row.iter().enumerate().find_map(|(x, cell)| {
            if (x, y) != base_pos && matches!(cell, Cell::Empty) {
                Some((x, y))
            } else {
                None
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_places_base_and_correct_dimensions() {
        let map = Map::new(20, 10);
        assert_eq!(map.width, 20);
        assert_eq!(map.height, 10);
        assert_eq!(map.cells.len(), 10);
        assert_eq!(map.cells[0].len(), 20);
        assert!(matches!(
            map.get(map.base_pos.0, map.base_pos.1),
            Cell::Base
        ));
    }

    #[test]
    fn generated_resources_use_required_quantity_range() {
        let map = Map::new(60, 25);
        let quantities: Vec<_> = map
            .cells
            .iter()
            .flatten()
            .filter_map(|cell| match cell {
                Cell::Resource(resource) => Some(resource.quantity),
                _ => None,
            })
            .collect();

        assert!(!quantities.is_empty());
        assert!(
            quantities
                .iter()
                .all(|quantity| (MIN_RESOURCE_QUANTITY..=MAX_RESOURCE_QUANTITY).contains(quantity))
        );
    }

    #[test]
    fn generated_base_area_stays_passable() {
        let map = Map::new(20, 10);
        for y in 0..map.height {
            for x in 0..map.width {
                if distance_from_base((x, y), map.base_pos) <= BASE_CLEAR_RADIUS {
                    assert!(!matches!(map.get(x, y), Cell::Obstacle));
                }
            }
        }
    }

    #[test]
    fn try_collect_depletes_and_clears_cell() {
        let mut map = Map::new(5, 5);
        map.cells[0][0] = Cell::Resource(Resource {
            kind: ResourceKind::Energy,
            quantity: 3,
        });

        assert_eq!(
            map.try_collect((0, 0), 2),
            Some((ResourceKind::Energy, 2, 1))
        );
        assert_eq!(
            map.try_collect((0, 0), 5),
            Some((ResourceKind::Energy, 1, 0))
        );
        assert!(matches!(map.get(0, 0), Cell::Empty));
    }
}

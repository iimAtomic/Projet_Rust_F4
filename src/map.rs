#[derive(Debug, Clone, PartialEq)]
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

impl Map {
    /// Stub: P1 will replace with Perlin-generated map
    pub fn new(width: usize, height: usize) -> Self {
        let base_pos = (width / 2, height / 2);
        let mut cells = vec![vec![Cell::Empty; width]; height];
        cells[base_pos.1][base_pos.0] = Cell::Base;
        Self { width, height, cells, base_pos }
    }

    pub fn get(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y][x]
    }

    pub fn is_passable(&self, x: usize, y: usize) -> bool {
        !matches!(self.get(x, y), Cell::Obstacle)
    }
}

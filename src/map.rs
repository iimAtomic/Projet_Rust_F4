use std::sync::{Arc, RwLock};

use noise::{NoiseFn, Perlin};
use rand::Rng;

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

/// La carte est partagée en lecture par tous les robots (pathfinding,
/// rendu) et en écriture par les collecteurs (prélèvement de ressource).
/// `RwLock` autorise plusieurs lecteurs concurrents ou un seul écrivain.
pub type SharedMap = Arc<RwLock<Map>>;

impl Map {
    /// Génération de carte par bruit de Perlin.
    ///
    /// Placeholder posé par P4 (intégration) pour que le reste de
    /// l'architecture (threads, BFS, UI) soit démontrable de bout en bout.
    /// **P1 est propriétaire de cette fonction** et peut librement changer
    /// l'algorithme de génération (seeds, biomes, densité de ressources...)
    /// tant que la signature `Map::new(width, height) -> Self` et les champs
    /// publics (`width`, `height`, `cells`, `base_pos`) restent compatibles.
    pub fn new(width: usize, height: usize) -> Self {
        let base_pos = (width / 2, height / 2);
        let perlin = Perlin::new(rand::thread_rng().r#gen());
        let mut rng = rand::thread_rng();

        let mut cells = vec![vec![Cell::Empty; width]; height];
        for y in 0..height {
            for x in 0..width {
                if (x, y) == base_pos {
                    continue;
                }
                let n = perlin.get([x as f64 * 0.15, y as f64 * 0.15]);
                cells[y][x] = if n > 0.45 {
                    Cell::Obstacle
                } else if n > 0.25 {
                    Cell::Resource(Resource {
                        kind: ResourceKind::Crystal,
                        quantity: rng.gen_range(3..=10),
                    })
                } else if n < -0.35 {
                    Cell::Resource(Resource {
                        kind: ResourceKind::Energy,
                        quantity: rng.gen_range(3..=10),
                    })
                } else {
                    Cell::Empty
                };
            }
        }
        cells[base_pos.1][base_pos.0] = Cell::Base;

        Self { width, height, cells, base_pos }
    }

    /// Wrappe une carte fraîchement générée dans le type partagé thread-safe.
    pub fn new_shared(width: usize, height: usize) -> SharedMap {
        Arc::new(RwLock::new(Self::new(width, height)))
    }

    pub fn get(&self, x: usize, y: usize) -> &Cell {
        &self.cells[y][x]
    }

    pub fn is_passable(&self, x: usize, y: usize) -> bool {
        !matches!(self.get(x, y), Cell::Obstacle)
    }

    /// Prélève atomiquement jusqu'à `amount` unités de ressource à `pos`.
    /// Retourne `Some((kind, prelevé, quantité_restante))` si une ressource
    /// était présente, `None` sinon (déjà épuisée par un autre collecteur,
    /// ou case sans ressource). Doit être appelé sous verrou d'écriture.
    pub fn try_collect(&mut self, pos: (usize, usize), amount: u32) -> Option<(ResourceKind, u32, u32)> {
        match &mut self.cells[pos.1][pos.0] {
            Cell::Resource(r) if r.quantity > 0 => {
                let taken = amount.min(r.quantity);
                r.quantity -= taken;
                let kind = r.kind.clone();
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
        assert!(matches!(map.get(map.base_pos.0, map.base_pos.1), Cell::Base));
    }

    #[test]
    fn is_passable_false_only_for_obstacles() {
        let mut map = Map::new(5, 5);
        map.cells[1][1] = Cell::Obstacle;
        map.cells[2][2] = Cell::Empty;
        assert!(!map.is_passable(1, 1));
        assert!(map.is_passable(2, 2));
    }

    #[test]
    fn try_collect_depletes_and_clears_cell() {
        let mut map = Map::new(5, 5);
        map.cells[0][0] = Cell::Resource(Resource { kind: ResourceKind::Energy, quantity: 3 });

        let first = map.try_collect((0, 0), 2).unwrap();
        assert_eq!(first, (ResourceKind::Energy, 2, 1));

        let second = map.try_collect((0, 0), 5).unwrap();
        assert_eq!(second, (ResourceKind::Energy, 1, 0));
        assert!(matches!(map.get(0, 0), Cell::Empty));

        assert!(map.try_collect((0, 0), 1).is_none());
    }
}

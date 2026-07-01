//! Contrat de communication entre les robots et la base.
//!
//! `RobotMessage` est le SEUL canal d'information autorisé entre un thread
//! robot (Scout ou Collector) et le thread Base. Toute nouvelle information
//! qu'un robot doit remonter passe par une nouvelle variante ici — ne pas
//! partager d'état mutable "en douce" entre modules.

use crate::map::ResourceKind;

/// Distingue le type de robot à l'origine d'un message (utile pour l'UI et
/// les stats, ex: afficher les scouts et collecteurs différemment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RobotKind {
    Scout,
    Collector,
}

#[derive(Debug, Clone)]
pub enum RobotMessage {
    /// Un scout (ou collecteur) a découvert une ressource à `pos`.
    ResourceFound {
        pos: (usize, usize),
        kind: ResourceKind,
        quantity: u32,
    },
    /// Un scout (ou collecteur) a découvert un obstacle à `pos`.
    ObstacleFound { pos: (usize, usize) },
    /// Un collecteur a prélevé `amount` unités de `kind` à `pos`.
    ResourceCollected {
        pos: (usize, usize),
        kind: ResourceKind,
        amount: u32,
    },
    /// Un robot a bougé vers une nouvelle position (pour l'affichage et le
    /// suivi de position par la base).
    RobotMoved {
        robot_id: usize,
        kind: RobotKind,
        pos: (usize, usize),
    },
}

# Projet_Rust_F4 — Essaim d'exploration

Simulation multi-robots en terminal (TUI) : une **base** coordonne des
**scouts** (exploration) et des **collecteurs** (récolte de ressources) sur
une carte procédurale, en visant deux ressources (`Energy`, `Crystal`).

```
cargo run
```

`q` ou `Échap` pour quitter.

## Rôles de l'équipe

| Module | Fichier | Propriétaire |
|---|---|---|
| Génération de carte (bruit de Perlin) | [`src/map.rs`](src/map.rs) | P1 |
| Exploration (Scout) | [`src/robot.rs`](src/robot.rs) (`Scout`) | P2 |
| Collecte & pathfinding (Collector) | [`src/robot.rs`](src/robot.rs) (`Collector`) | P3 |
| Contrat de messages, boucle Base, architecture concurrente, intégration | [`src/messages.rs`](src/messages.rs), [`src/base.rs`](src/base.rs), [`src/main.rs`](src/main.rs) | P4 (moi) |
| Rendu TUI | [`src/ui.rs`](src/ui.rs) | — |

`Map::new` et `Scout::step` contiennent actuellement des implémentations
minimales (Perlin basique / marche aléatoire) posées pour que
l'architecture soit démontrable de bout en bout dès maintenant. Elles sont
clairement marquées `Placeholder` dans le code — P1 et P2 peuvent les
remplacer librement **sans toucher au reste du projet**, tant que les
signatures ci-dessous restent stables :

- `Map::new(width: usize, height: usize) -> Map`, avec les champs publics
  `width`, `height`, `cells: Vec<Vec<Cell>>`, `base_pos`.
- `Scout::step(&mut self, map: &SharedMap)` — ne communique avec le reste du
  système *que* via `RobotMessage` envoyés sur le channel.

## Le contrat : `RobotMessage`

Défini dans [`src/messages.rs`](src/messages.rs), c'est le **seul** canal
d'information autorisé entre un thread robot et le thread Base. Un robot ne
doit jamais muter un état partagé "en douce" — il envoie un message, la
Base agrège :

```rust
enum RobotMessage {
    ResourceFound   { pos, kind, quantity },  // scout/collecteur découvre une ressource
    ObstacleFound   { pos },                  // scout/collecteur découvre un obstacle
    ResourceCollected { pos, kind, amount },   // collecteur prélève une ressource
    RobotMoved      { robot_id, kind, pos },  // suivi de position pour l'UI
}
```

## Architecture concurrente

**Choix : threads OS + channels (`crossbeam-channel`)**, pas d'async/tokio —
le nombre de robots est petit et fixe, chaque robot a une boucle bloquante
simple (`sleep` + calcul), un modèle à threads reste le plus lisible ici.

```
┌─────────┐  RobotMessage   ┌──────────────┐
│ Scout(s)│ ───────────────►│              │
├─────────┤   (mpsc via     │  Thread Base │──► known_map (Arc<RwLock<HashMap>>)
│Collector│  crossbeam-     │  (1 seul     │──► ui_state  (Arc<Mutex<UiState>>)
│  (s)    │◄─── channel) ───│  receiver)   │
└────┬────┘                 └──────────────┘
     │  lecture/écriture
     ▼
  Map (Arc<RwLock<Map>>) ── vérité terrain, partagée par tous
```

- **`SharedMap` (`Arc<RwLock<Map>>`)** : vérité terrain. Lue par tout le
  monde (scouts sentent leur voisinage, collecteurs vérifient une case,
  rendu), écrite brièvement par les collecteurs (`Map::try_collect`, verrou
  d'écriture) lors d'une récolte — atomique, donc deux collecteurs ne
  peuvent pas prélever la même dernière unité.
- **`SharedKnownMap` (`Arc<RwLock<HashMap<(usize,usize), Cell>>>`)** :
  connaissance agrégée (fog of war), **seule la Base y écrit**, à partir des
  messages `ResourceFound`/`ObstacleFound`/`ResourceCollected`. Les
  collecteurs la lisent (snapshot cloné) pour le BFS de pathfinding — c'est
  ce qui permet à un collecteur de profiter des découvertes d'un scout sans
  lien direct entre eux.
- **`SharedUi` (`Arc<Mutex<UiState>>`)** : uniquement écrit par la Base
  (compteurs + positions), lu par le thread de rendu.
- **Arrêt** : un `Arc<AtomicBool>` partagé. Sur `q`/`Échap`, le thread de
  rendu le positionne à `true` ; chaque thread robot le vérifie à chaque
  tick, la Base le vérifie après chaque `recv_timeout` (50 ms) pour rester
  réactive sans busy-loop. `main` attend la fin de tous les threads
  (`join`) avant de restaurer le terminal, pour ne perdre aucun message en
  vol.

## Gestion des erreurs

- Le terminal (mode brut + écran alternatif) est restauré **même en cas de
  panic** dans `run()` : `main` encapsule l'exécution dans
  `catch_unwind`, restaure toujours le terminal, puis relance le panic
  (`resume_unwind`) pour ne pas masquer un vrai bug.
- Les verrous (`Mutex`/`RwLock`) sont "poison-safe" : si un thread panique
  en tenant un verrou, les autres le récupèrent via
  `unwrap_or_else(|e| e.into_inner())` plutôt que de paniquer en cascade.
- Un `send()` sur un channel dont la Base a disparu échoue silencieusement
  (`let _ = ...`) — c'est le comportement attendu pendant l'arrêt.

## Tests

```
cargo test
```

11 tests couvrent :
- `map.rs` : dimensions/position de la base, `is_passable`, `try_collect`
  (prélèvement partiel, épuisement, case déjà vide).
- `base.rs` : agrégation des découvertes dans `known_map`, mise à jour des
  compteurs globaux + dépeuplement de `known_map` sur `ResourceCollected`,
  synchronisation des positions vers `UiState`.
- `robot.rs` : BFS (chemin direct, contournement d'obstacle connu, cible
  inatteignable), cycle complet d'un collecteur (idle → aller → collecte →
  retour → idle), un scout qui signale une ressource voisine et se déplace.

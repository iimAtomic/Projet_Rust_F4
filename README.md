# Projet_Rust_F4

Simulation multi-threadée en terminal (Ratatui) d'une flotte de robots qui explorent
une carte procédurale et rapportent des ressources (énergie / cristaux) vers une base
centrale.

```bash
cargo run
```

Appuyez sur n'importe quelle touche pour quitter la simulation.

## Sommaire

- [Concept](#concept)
- [Architecture](#architecture)
- [Modules](#modules)
- [Cycle de vie d'un tick](#cycle-de-vie-dun-tick)
- [Constantes de configuration](#constantes-de-configuration)
- [Interface / légende](#interface--légende)
- [Commandes utiles](#commandes-utiles)

## Concept

Une carte de `60x25` cases est générée avec du bruit de Perlin pour placer des
obstacles, une base au centre, et des ressources (`Energy` / `Crystal`, 50 à 200
unités chacune) réparties aléatoirement sur les cases libres.

Deux types de robots tournent chacun sur leur propre thread OS :

- **Scout** : se déplace aléatoirement de case en case, observe son voisinage et
  signale à la base tout ce qu'il découvre (obstacle, ressource, case vide/base).
- **Collector** : n'agit que sur la base de ce que la base connaît déjà (fog of
  war) ; calcule un chemin en largeur (BFS) jusqu'à la ressource connue la plus
  proche, la collecte petit à petit jusqu'à saturation de sa cargaison, puis
  revient à la base pour la vider avant de recommencer.

Tant qu'il reste au moins une ressource sur la carte, la simulation continue.
Une fois toutes les ressources épuisées, l'affichage bascule sur
`Simulation terminee`.

## Architecture

Les robots ne partagent jamais leur état directement : ils communiquent avec un
thread **Base** unique via un canal MPSC (`crossbeam-channel`), en s'échangeant des
messages typés (`RobotMessage`). C'est la Base qui est seule responsable de mettre
à jour la carte "connue" (fog of war) et les statistiques affichées.

```
 ┌─────────┐  RobotMessage   ┌──────────┐
 │ Scout xN│ ───────────────▶│          │
 └─────────┘                 │   Base   │──▶ known_map (Arc<RwLock<HashMap>>)
 ┌─────────┐  RobotMessage   │ (thread) │──▶ UiState   (Arc<Mutex<UiState>>)
 │Collect xN│──────────────▶ │          │
 └─────────┘                 └──────────┘
       │                                          ▲
       │ lit/écrit                                │ lit pour dessiner
       ▼                                          │
 ┌───────────────────────┐                 ┌──────────────┐
 │ Map (Arc<RwLock<Map>>)│◀────────────────│ Boucle de rendu│
 └───────────────────────┘                 └──────────────┘
```

État partagé entre threads :

| État                | Type                                      | Rôle |
|---------------------|--------------------------------------------|------|
| `shared_map`        | `Arc<RwLock<Map>>`                          | Carte réelle (obstacles, ressources, quantités) |
| `known_map`          | `Arc<RwLock<HashMap<(x,y), Cell>>>`         | Ce que la base sait déjà (fog of war) |
| `shared_ui`          | `Arc<Mutex<UiState>>`                       | Compteurs + positions des robots pour l'affichage |
| `stop`               | `Arc<AtomicBool>`                           | Signal d'arrêt partagé (fin de simulation ou touche pressée) |

Le `main` protège aussi le rendu avec `catch_unwind` : si la boucle de simulation
panique, le terminal (raw mode, alternate screen) est quand même restauré
proprement avant de repropager le panic.

## Modules

| Fichier          | Rôle |
|-------------------|------|
| `src/main.rs`     | Point d'entrée : configuration du terminal, lancement des threads (base, scouts, collectors), boucle de rendu et de fin de partie. |
| `src/map.rs`      | Génération procédurale de la carte (Perlin + placement de ressources), type `Cell`/`Resource`/`Map`, logique de collecte (`try_collect`). |
| `src/base.rs`     | Thread `Base` : reçoit les `RobotMessage`, met à jour `known_map` et les compteurs, synchronise `UiState`. |
| `src/robot.rs`    | Logique des robots : `Scout` (exploration aléatoire) et `Collector` (machine à états + BFS). |
| `src/messages.rs` | Protocole de communication `RobotMessage` / `RobotKind` échangé sur le canal `crossbeam-channel`. |
| `src/ui.rs`       | Rendu Ratatui : carte colorée, panneau de statistiques, légende, `UiState`. |

## Cycle de vie d'un tick

**Scout** (`Scout::step`) :
1. Regarde les 4 cases voisines non encore signalées.
2. Pour chacune : envoie `ObstacleFound`, `ResourceFound` ou `CellSeen` selon le contenu.
3. Choisit aléatoirement une case voisine praticable et s'y déplace (`RobotMoved`).

**Collector** (`Collector::step`), machine à états `CollectorState` :
- `Idle` → si la cargaison est pleine : rentre à la base ; sinon cherche la
  ressource connue la plus proche (`select_target`) et calcule un chemin BFS
  (`bfs_path`) → passe en `MovingToResource`.
- `MovingToResource(path)` → avance d'une case par tick ; une fois arrivé,
  passe en `Collecting`.
- `Collecting { pos }` → prélève 1 unité par tick (`Map::try_collect`), envoie
  `ResourceCollected` ; repart vers la base si la cargaison est pleine ou la
  ressource épuisée.
- `ReturningToBase(path)` → avance d'une case par tick ; à l'arrivée, vide sa
  cargaison (`cargo = 0`) et repasse en `Idle`.

**Base** (`Base::run`) : boucle bloquante sur `recv_timeout(50ms)` ; à chaque
message reçu, met à jour `known_map` et/ou les compteurs `energy_collected` /
`crystals_collected`, puis pousse l'état à jour dans `UiState`. S'arrête quand le
canal est fermé ou que `stop` est levé.

**Boucle de rendu** (`main::render_loop`) : à chaque itération, vérifie s'il
reste des ressources sur la carte réelle (`Map::has_resources`), dessine la
carte + le panneau de stats, et écoute le clavier (poll 100ms) pour quitter.

## Constantes de configuration

Définies en tête de `src/main.rs` :

| Constante            | Valeur | Effet |
|------------------------|--------|-------|
| `MAP_WIDTH` / `MAP_HEIGHT` | 60 / 25 | Dimensions de la carte |
| `SCOUT_COUNT`           | 2      | Nombre de robots éclaireurs |
| `COLLECTOR_COUNT`       | 2      | Nombre de robots collecteurs |
| `COLLECTOR_CAPACITY`    | 5      | Cargaison max avant retour à la base |
| `ROBOT_TICK`            | 150 ms | Délai entre deux actions d'un robot |

## Interface / légende

Le panneau de droite affiche en direct :

- **Energy** / **Crystals** : quantités totales collectées et rapportées à la base.
- **Known** : nombre de cases découvertes (taille de `known_map`).
- Le statut de la simulation (`Simulation active` / `Simulation terminee`).

Symboles sur la carte :

| Symbole | Couleur         | Signification |
|---------|-----------------|----------------|
| `#`     | vert clair       | Base |
| `E`     | vert             | Ressource énergie |
| `C`     | magenta clair    | Ressource cristal |
| `O`     | cyan clair       | Obstacle |
| `.`     | gris foncé       | Case vide déjà explorée |
| `?`     | gris foncé       | Case inconnue (hors du fog of war) |
| `x`     | rouge            | Robot scout |
| `o`     | magenta          | Robot collecteur |

## Commandes utiles

```bash
cargo run              # lance la simulation
cargo test              # tests unitaires (génération de carte, collecte, symboles UI)
cargo fmt --check        # vérifie le formatage
cargo clippy --all-targets   # lint (0 warning à date)
```

`cargo test` couvre notamment : dimensions et placement de la base, bornes des
quantités de ressources générées, praticabilité de la zone autour de la base,
comportement de `try_collect` (dépletion + nettoyage de case), et le mapping
case → symbole/couleur de l'UI.

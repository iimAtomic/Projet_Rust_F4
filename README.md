# Projet_Rust_F4 - Partie P1

Simulation terminal Ratatui d'une carte de collecte de ressources.

```bash
cargo run
```

Appuyez sur n'importe quelle touche pour quitter.

## Partie personne 1

- Generation de carte avec obstacles issus du bruit de Perlin.
- Placement de ressources `E` energie et `C` cristal sur cases libres.
- Quantites de ressources entre 50 et 200 unites.
- Base centrale `#` et zone de depart degagee.
- Rendu Ratatui couleur conforme au sujet.
- Fog of war: seules les cases connues via `known_map` sont affichees, les autres restent `?`.
- Message `Simulation terminee` quand toutes les ressources sont epuisees.

## Tests utiles

```bash
cargo fmt --check
cargo test
cargo run
```

`cargo test` verifie notamment la generation de carte, les quantites de
ressources et les symboles utilises par l'affichage.

## Couleurs

- `O`: obstacle, cyan clair.
- `E`: energie, vert.
- `C`: cristal, magenta clair.
- `#`: base, vert clair.
- `x`: scout, rouge.
- `o`: collecteur, magenta.
- `?`: case inconnue.

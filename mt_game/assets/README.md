# Sprites

`sprites.png` is a 128x128 sheet of 62 cells (16x16) cut from five packs by Kenney, all released under CC0 1.0. Only the cells the demo
draws are kept; `src/art.rs` says which cell is which. Each pack's licence text, as shipped in the pack, sits next to this file.

| Pack | Page | Licence file | Used for |
| --- | --- | --- | --- |
| Tiny Battle | https://opengameart.org/content/tiny-battle | `LICENSE-kenney-tiny-battle.txt` | terrain, cores, buildings, units, grass sprouts |
| Tiny Town | https://opengameart.org/content/tiny-town | `LICENSE-kenney-tiny-town.txt` | ore nugget, ruin, trees, bush and mushrooms in the margins |
| Tiny Dungeon | https://opengameart.org/content/tiny-dungeon | `LICENSE-kenney-tiny-dungeon.txt` | crater |
| Tiny Ski | https://opengameart.org/content/tiny-ski | `LICENSE-kenney-tiny-ski.txt` | the chunky digit and capital-letter font |
| Pixel Shmup | https://opengameart.org/content/pixel-shmup | `LICENSE-kenney-pixel-shmup.txt` | clash effects: bursts, spark, smoke |

The sheet is rebuilt by a script that is not part of the repo; these are the changes it makes to the pack art:

- Tiny Battle's blue units and buildings are hue-shifted to the game's teal (orange stays as it is). The drone is the single-rotor
  helicopter (11,r), the flak is the antenna tower (13,r), the archer is the grunt's second pose plus a bow badge.
- Archer: the bow badge is drawn by hand (7x9 px, wood arc, white string, the packs' plum outline). Tiny Town's bow icon (10,9)
  is 11x16 px and loses its wood when halved.
- Trench: drawn by hand, a dug ditch with a shadowed far wall and a lit near lip, in the dirt colours the Kenney packs share
  (the same ones as Tiny Farm's furrow). Neither the furrow nor Tiny Town's plain dirt reads as dug-in ground.
- Ruin (Tiny Town 7,3): the rubble stones get a one-pixel plum outline, because the pack's version is faint on grass.
- Effects (Pixel Shmup 4,0 to 8,0): the pack draws these without an outline; they get the plum one (#3f2631) the other sprites have.
- Font (Tiny Ski rows 7 to 10): digits, `-`, `!` and the letters the game uses are copied as they are. The pack has no slash, so
  `/` is drawn by hand in the same style. The game draws each glyph from its inked columns only, so the text is proportional.
- Tiny Battle's grass-with-sprouts cell (1,0) is a quiet variant of the grass tile, chosen by tile index.

Tried and left out: UI Pack Pixel Adventure (9-slice boxes and buttons) made the panel busier and its warm brown borders sit
too close to the enemy's orange, so the panel keeps its flat boxes. Flowers, the autumn tree and fences from Tiny Town were left
out too (see `draw::margin_decor` and `draw::grass_cell` for why).

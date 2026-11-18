# Cube Game

An arena shooter in which the game window itself is the arena – and it keeps shrinking. Shoot its edges to push them
back out. A program of the [Windows Toolbox](https://github.com/Windows-Toolbox/Toolbox).

> **Downloaded from the releases?** The `.exe` is not signed (signing costs money). Windows SmartScreen may warn
> once at the first start: *More info → Run anyway*.

## Rules for Claude (read first)

These rules hold on every PC and in every session; they are here because `CLAUDE.md` and Claude's memory stay local.

- **The game is Maximilian's learning project.** He writes game code himself; Claude advises, explains, reviews,
  gives hints first and solutions only when asked. Small changes only on request, with an explanation.
- **Exception:** the connection to the Toolbox (`src/main.rs`, `Cargo.toml`, `build.ps1`, `release.ps1`) is
  infrastructure and maintained like the rest of the Toolbox.
- **The Rust version was ported by Claude on request** (2026-11-18): the Python version, 1:1 – same behaviour, same
  data files, same texts. That was a one-time job; new game content falls under the first rule again.
- Keep the checklist below and [HISTORY.md](HISTORY.md) (the Python version's development log) up to date.

## Using it

Add the folder in the Toolbox (**Add program…**) and press **Play** – or double-click `dist\CubeGame.exe`. The
Toolbox page has the two settings (key display at the start, arena on all monitors) and explains how to use your own
enemy pictures. **Stop** in the Toolbox closes the game.

Controls: W A S D move · mouse aim · left click shoot · right click or mouse wheel switch weapons · Shift dash ·
Space rocket · B watchtower · Tab inventory · T shop · Esc menu · F3 key display.

What you keep between runs is in `dist\data\profile.json` (cores, skill tree, cards, loadout, best times); the page's
settings in `dist\data\settings.json`; your enemy pictures in `dist\data\images\`.

## Building

`build.ps1` builds the release and installs it into `dist\` (needs Rust). Changing the game's numbers needs no build
on this PC: the files in `content\` are read from here every time the game menu opens (`content\README.md` explains
every value); a release has them compiled in. `cargo test` runs the tests, all without a window – among them a bot
that plays every level for 2½ minutes with the whole skill tree and draws every frame into memory.
`cargo test pictures -- --ignored` saves pictures of the menus and a run to the temp folder.

## License

MIT (see [LICENSE](LICENSE)) – except `assets/freesansbold.ttf`, pygame's built-in font from GNU FreeFont, which keeps
its own license (GPL with font exception: it may be embedded in any program).

## The files

| Path | What |
|---|---|
| `src/main.rs` | the connection to the Toolbox: settings, page, Play / Stop |
| `src/engine/` | everything not specific to this game: `window.rs` (layered windows, extra windows, mouse lock, input), `gfx.rs` (drawing on tiny-skia, with pygame's rules), `font.rs`, `geom.rs` (pygame's Rect/FRect/Vector2), `mod.rs` (the game loop, scenes), `menu.rs`, `ui.rs`, `key_overlay.rs`, `collision.rs`, `desktop.rs`, `sprite_cache.rs` |
| `src/game/` | the game: scenes (`scenes.rs`, `game_menu.rs`, `play_scene.rs`, `game_over.rs`), `panels.rs` (the game menu's tabs), `ship.rs`, `weapons.rs`, `abilities.rs`, `enemies.rs` (all types and their looks), `spawner.rs`, `roster.rs`, `arena.rs`, `field.rs`, `points.rs`, `radar.rs`, `buildings.rs`, `effects.rs`, `projectile.rs`, `pickups.rs`, `hud.rs`, `shop.rs`, `cards.rs`, `skill_tree.rs`, `profile.rs`, `difficulty.rs`, `tables.rs` (ships, abilities, weapons, radar, points, buildings), `shop_data.rs`, `settings.rs`, `shapes.rs` |
| `content/` | the data files (ships, weapons, enemies, levels, …) – changed without compiling |
| `assets/` | `i18n.json` (the Toolbox page's texts), `freesansbold.ttf` (pygame's built-in font, GNU FreeFont – GPL with font exception) |

## Port – checklist

Written from the Python code (`Toolbox\programs\cube_game\`, until 2026-11-18) before the port, ticked off during it.
✓ ported and checked (test, or compared with the Python version), ◐ ported, not checked by hand yet.

Engine
- ✓ Fixed timestep, 60 ticks/s; frames synced with `DwmFlush`, at most 144 fps; at most 0.25 s caught up after a freeze
- ✓ Window position, size and picture in one step (`UpdateLayeredWindow`), drawn straight into the window's memory;
  the bitmap only grows
- ◐ Extra windows: borderless, no taskbar button, owned by the main window, never activated (`WM_MOUSEACTIVATE`);
  a focus loss to one of them is taken back without the scene noticing
- ◐ Mouse lock (`ClipCursor`) into the field's bounding box, back onto the nearest rectangle in a gap; let go on focus
  loss; Alt+F4 / close ends the game
- ✓ pygame's drawing rules: shapes replace pixels, blits mix, outlines inside, no smoothing; text in pygame's font and
  sizes (`Font(None, n)` = FreeSansBold at 0.6875·n) – checked: menus and skill tree pixel-close to the Python version
- ✓ Monitors: work areas, `covers` with gaps, all monitors or only the main one (setting)
- ✓ Key display (F3), menus (mouse + W/S/arrows + Enter/Space, release fires), context menu, picture cache

Game
- ✓ Main menu, game menu (Levels, Loadout with drag & drop, Skill tree with tooltips, Cards with offers, reroll,
  collection, activate, sell via right click), game over (input delay), pause menu, inventory, shop
- ✓ Data files with every check and warning text of the Python version; all files load without warnings (test)
- ✓ Arena: shrinking (growth, maximum, phases), pushes (armor, falloff, push speed, only onto screen), hits biased away
  from the ship, breathing, storm, shove (Pusher), crush – the first two seconds of a run match Python to the decimal
- ✓ Ship: movement, aim, energy, armor, recovery, magnet, knockback and stun, two weapon slots and switching
- ✓ Weapons: gun, machine gun (spread), cannon (pierce, pierced push), laser (bolts drawn as a beam); skill tree
  upgrades and side effects; shop upgrades per weapon; cards with upgrade factors
- ✓ Abilities: dash (charges, invulnerability, afterimages, mines, slow field), rocket (acceleration, blast,
  burning ground)
- ✓ All 15 enemy types with their behaviour and looks (basic, charger, shooter, sniper, carrier, critter, artillery,
  fat, cannoneer, shield, laser, cluster, commander, pusher, living window), tiers, buffs, auras, swarms with leaders,
  evolutions, separation by mass, lure, slow, damage factor
- ✓ Spawner: weights with growth, phases (weights, buffs, announce, arena, spawn, spawn interval, breathe, points,
  one_of, every), swarms away from all windows, Pushers beyond the map
- ✓ Field: connected windows, shots into windows, blast walls, mouse area, sight, outline of all windows
- ✓ Watchtowers (throw, flight, window, lifetime, damage bonus, lure), radar (groups, finds), points of interest
  (chest, magnet, repair, totem, nest, storm) with edge markers
- ✓ Stars and cores, the shop's prices, level goal, unlock, best times, notices, off-screen markers, swarm markers
- ◐ Own enemy pictures (`data\images\<enemy>.png`, turned in 5° steps, white flash) – not tried with a real picture
- ◐ How it plays and feels by hand (speed, flicker, mouse, several monitors)

Toolbox
- ✓ Kit program without background role: page with the two settings and the pictures button, Play / Stop, a
  second start brings the window to the front, `bye` after the game closes (kit: `Runtime::stop`)
- ◐ Hand test: Play, Stop and the page in the real Toolbox

Changed on purpose
- The progress file is `data\profile.json` next to the exe, the pictures folder
  `data\images\` (was `assets\images\` in the program folder) – both are yours, never touched by a build or update.
- Random numbers come from an own generator (not Python's) – a run never played the same twice anyway.
- If Windows refuses the layered window, the Python version fell back to SDL's flickering way; this one runs without a
  window then (it has not happened on any PC so far).

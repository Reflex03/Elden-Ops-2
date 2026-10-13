# Elden Ops

**Black Ops II Zombies rounds in the Lands Between.** A solo Elden Ring mod: you load in at a Site of Grace,
Black Ops II's round-start music plays, and the enemies around you become Round 1's horde. Every hit and kill
earns BO2 points; clear the horde and the next, tougher round begins. Spend your points on Perk-a-Colas, with
every jingle, sting and cha-ching played from your own copy of Black Ops II Zombies.

## What you get
- **Endless Zombies rounds in Elden Ring.** Round size follows BO2's own solo formula (6, 8, 13, 18, 24, 27 … kills),
  and enemies near you get tougher each round along BO2's zombie-health curve (softened for Elden Ring).
- **BO2 points:** 10 per hit, 50 per kill, starting at 500 (BO2's values).
- **Four Perk-a-Colas** at BO2's prices, bought with keys:
  | Key | Perk | Cost | In Elden Ring |
  |---|---|---|---|
  | F5 | Juggernog | 2500 | BO2 health 100 → 250: a pool of +150% max HP absorbs damage, refilled each round |
  | F6 | Speed Cola | 3000 | stamina recovers much faster |
  | F7 | Double Tap Root Beer | 2000 | your hits deal double damage |
  | F8 | Quick Revive | 500 | a lethal hit stands you back up at full HP, once (you lose your perks, as when downed in BO2) |
- **Black Ops II's real audio:** round start / round over / game over music, perk stings, machine jingles between
  rounds, the cha-ching and the deny buzz, played straight from your BO2 sound banks.
- **BO2-style HUD:** red round number, points, perk shelf, GAME OVER / YOU SURVIVED N ROUNDS.
- Dying without Quick Revive is game over: your next life starts again at Round 1 with 500 points.
- Resting at a Site of Grace brings the Lands Between's enemies back, refilling the horde.

## Games you need
- **Elden Ring** 1.17.1 (game version 2.7.1.0, or 2.7.1.1 in Japan) — the game you play. Melty starts it through
  ModEngine2, offline, with Easy Anti-Cheat off. Use a separate save if you also play online.
- **Call of Duty: Black Ops II – Zombies** installed (Steam). It doesn't run: Elden Ops reads its sounds and its
  alias tables from your copy.

Solo only.

## How it works
- `mod/eldenops` — a native DLL that ModEngine2 loads into Elden Ring (`package/config_eldenops.toml`). It reads
  and writes Elden Ring through [fromsoftware-rs](https://github.com/vswarte/fromsoftware-rs), draws the HUD with
  [hudhook](https://github.com/veeenu/hudhook) and plays audio with [rodio](https://github.com/RustAudio/rodio).
- `mod/eldenops-setup` — runs once on first Play. It asks [OpenAssetTools](https://github.com/Laupetin/OpenAssetTools)'
  Unlinker for the **names** in your BO2 Zombies fastfiles' sound alias tables (`--skip-obj`: no audio is loaded),
  then finds each sound in your `.sabl`/`.sabs` banks by the game's own name hash. It writes a small index
  (`%LOCALAPPDATA%\EldenOps\bo2_index.json`: names, bank file, offset, size). No BO2 audio is copied anywhere;
  the DLL reads it from your banks each time the game starts.
- `mod/bo2snd` — the BO2 (T6) sound bank reader both use.
- `design/*.json` — the design sheets (rounds, scoring, perks, sounds, hooks, systems, HUD). They are the source of
  truth: `python3 tools/sheets.py preflight` checks every cell and cross-reference, `python3 tools/sheets.py gen`
  regenerates `mod/eldenops/src/sheets.rs` and the setup's alias list.

## Build
```
python3 tools/sheets.py gen
cd mod && cargo test -p bo2snd -p eldenops        # round logic + sound bank tests (any OS)
cargo xwin build --release --target x86_64-pc-windows-msvc -p eldenops -p eldenops-setup
cd .. && ./tools/fetch_vendor.sh && python3 tools/package.py
```
Logs: `%LOCALAPPDATA%\EldenOps\eldenops.log` and `setup.log`.

## Credits
- Black Ops II Zombies rules, sound alias names: Treyarch's own scripts and data, read from your copy.
- [OpenAssetTools](https://github.com/Laupetin/OpenAssetTools) by Laupetin and contributors (GPL-3.0), bundled unmodified.
- [fromsoftware-rs](https://github.com/vswarte/fromsoftware-rs) by Vincent Swarte and contributors.
- [hudhook](https://github.com/veeenu/hudhook) by Andrea Venuta; [rodio](https://github.com/RustAudio/rodio)/claxon by the RustAudio contributors.
- [ModEngine2](https://github.com/soulsmods/ModEngine2) (installed by Melty).
- T6 sound bank layout: OpenBO2 / OpenAssetTools; itsmeft24's T6-Sound-Replacement-Plugin.
- Microsoft Visual C++ runtime (x86, app-local, for OpenAssetTools).
- Built with Claude Code and the universal-modder toolkit.

Elden Ops' own code is MIT-licensed and open for remixes. Elden Ring © Bandai Namco / FromSoftware; Call of Duty:
Black Ops II © Activision. Not affiliated with either.

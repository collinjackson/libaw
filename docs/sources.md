# Sources

Where libaw's rules and data come from, under what terms, and what each is used for.

- **AWBW replay recordings.** Games fetched from AWBW's replay API (`load_replay.php`) and saved
  by `awbw_chart/awbw_replay_export.py`.
  - Terms: AWBW's data; private use only.
  - Used for: the ground truth for every rule.
- **[AWBW damage chart](https://awbw.amarriner.com/damage.php).**
  - Terms: AWBW's page, fetched once.
  - Used for: `data/damage.json`.
- **[AWBW-Replay-Player](https://github.com/DeamonHunter/AWBW-Replay-Player).** Its
  `AWBWApp.Resources/Json/` files for tiles, buildings, units and COs; the commit used is in
  `data/replay-player/COMMIT`.
  - Terms: MIT (`data/replay-player/LICENSE`).
  - Used for: terrain ids, movement costs, defence and unit stats.
- **AWBW wiki.**
  - Terms: CC BY-SA (fandom).
  - Used for: rule descriptions, not copied text.
- **Rule clarifications.** Plain-language descriptions of behaviour and test cases, with no AWBW
  source code.
  - Terms: shared privately for this project.
  - Used for: details recorded games can't settle, such as damage rounding, counterattack bonuses
    and rarely used CO powers (from 2026-11-11). The full list, fixed and open, is in the
    [rule findings](awbw-rule-findings.md).

Not used, by rule: `carsonhu/awbw-ai` (clean room, see
[CLAUDE.md](https://github.com/collinjackson/libaw/blob/main/CLAUDE.md)) and `DefendPeace` (no
license).

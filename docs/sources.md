# Sources

| Source | License | Used for |
|---|---|---|
| AWBW replay API (`load_replay.php`) recordings, saved by `awbw_chart/awbw_replay_export.py` | AWBW's data; private use only | Ground truth for every rule |
| [AWBW damage chart](https://awbw.amarriner.com/damage.php) | AWBW's page, fetched once | `data/damage.json` |
| [AWBW-Replay-Player](https://github.com/DeamonHunter/AWBW-Replay-Player) `AWBWApp.Resources/Json/{Tiles,Buildings,Units,COs}.json`, commit in `data/replay-player/COMMIT` | MIT (`data/replay-player/LICENSE`) | Terrain ids, movement costs, defence, unit stats |
| AWBW wiki | CC BY-SA (fandom) | Rule descriptions, not copied text |

Not used, by rule: `carsonhu/awbw-ai` (clean room, see CLAUDE.md); `DefendPeace` (no license).

"""Extracts the official level files from a Distance installation, for `OFFICIAL_LEVELS_DIR`.

The levels are TextAssets in the game's `resources.assets`, named after the level. This reads
the official level names from `official_level_names.rs` of distance-util, and writes each level
to `<out_dir>/<level name>.bytes`.

Usage (needs UnityPy: `pip install UnityPy`):

    python extract_official_levels.py <Distance_Data dir> <official_level_names.rs> <out_dir>

For example, with the game in the default Steam library on Windows, from WSL:

    python extract_official_levels.py \
        "/mnt/c/Program Files (x86)/Steam/steamapps/common/Distance/Distance_Data" \
        ~/.cargo/git/checkouts/distance-util-*/*/src/official_level_names.rs \
        official-levels
"""

import os
import re
import sys

import UnityPy


def main():
    game_data_dir, names_file, out_dir = sys.argv[1:4]

    with open(names_file, encoding="utf-8") as f:
        names = sorted(set(re.findall(r'^\s*"([^"]+)",\s*$', f.read(), re.MULTILINE)))
    wanted = set(names)

    found = {}
    env = UnityPy.load(os.path.join(game_data_dir, "resources.assets"))
    for obj in env.objects:
        if obj.type.name != "TextAsset":
            continue
        asset = obj.read()
        if asset.m_Name not in wanted:
            continue
        script = asset.m_Script
        data = script.encode("utf-8", "surrogateescape") if isinstance(script, str) else bytes(script)
        found.setdefault(asset.m_Name, []).append(data)

    os.makedirs(out_dir, exist_ok=True)
    problems = 0
    for name in names:
        files = found.get(name, [])
        if len(files) != 1:
            print(f"{name}: found {len(files)} assets", file=sys.stderr)
            problems += 1
        if files:
            with open(os.path.join(out_dir, name + ".bytes"), "wb") as f:
                f.write(files[0])

    print(f"Extracted {sum(1 for n in names if n in found)} of {len(names)} official levels")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Builds dist/EldenOps-<version>.zip. Its modengine2/ folder installs to {managed}/modengine2."""
import hashlib
import json
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSION = "0.1.6"
REL = ROOT / "mod" / "target" / "x86_64-pc-windows-msvc" / "release"
OAT = ROOT / "vendor" / "oat"
ME2 = ROOT / "vendor" / "me2" / "ModEngine-2.1.0.0-win64"


def third_party_notices() -> str:
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc"],
        cwd=ROOT / "mod"))
    used = {n["id"] for n in meta["resolve"]["nodes"]}
    ws = set(meta["workspace_members"])
    lines = ["Elden Ops bundles the following third-party software.", "",
             "== OpenAssetTools v0.33.0 (oat/Unlinker.exe) ==",
             "By Laupetin and contributors. GNU General Public License v3.0 (oat/LICENSE-OpenAssetTools.txt).",
             "Bundled unmodified. Corresponding source: https://github.com/Laupetin/OpenAssetTools/tree/v0.33.0", "",
             "== ModEngine2 release-2.1.0 (modengine2_launcher.exe, modengine2/) ==",
             "By the soulsmods contributors. MIT License (LICENSE-ModEngine2.txt). https://github.com/soulsmods/ModEngine2", "",
             "== Microsoft Visual C++ Runtime 14.44, x86 (oat/) and x64 (beside ModEngine2's binaries) ==",
             "Redistributable files from Microsoft Visual Studio 2022 (Microsoft.VC143.CRT), distributed app-local",
             "under the Visual Studio redistribution terms. (c) Microsoft Corporation.", "",
             "== Rust crates compiled into eldenops.dll and EldenOpsSetup.exe ==", ""]
    for p in sorted((p for p in meta["packages"] if p["id"] in used and p["id"] not in ws), key=lambda p: p["name"]):
        lic = p.get("license") or "see its LICENSE file"
        if p["name"] == "hudhook":
            lic = "MIT (Copyright (c) 2020-2024 Andrea Venuta)"
        if p["name"] in ("eldenring", "fromsoftware-shared", "fromsoftware-shared-stl", "fromsoftware-shared-macros"):
            lic = "MIT NON-AI License (Copyright (c) 2025 Vincent Swarte) - https://github.com/vswarte/fromsoftware-rs"
        repo = p.get("repository") or ""
        lines.append(f"{p['name']} {p['version']}: {lic} {repo}".rstrip())
    return "\n".join(lines) + "\n"


PLAYER_README = """Elden Ops {v}: Black Ops II Zombies rounds in the Lands Between

Press Play in Melty. On the first Play, Melty reads Black Ops II Zombies' sound names from your own copy
(a few seconds to a minute), then starts Elden Ring offline through ModEngine2.

Load your character. Round 1 starts at the grace: the enemies around you are the horde.
  Points: 10 per hit, 50 per kill. You start with 500.
  F5 Juggernog 2500  F6 Speed Cola 3000  F7 Double Tap 2000  F8 Quick Revive 500
  Die without Quick Revive: game over, next life starts again at Round 1.
  Rest at a Site of Grace to bring enemies back.

Logs: %LOCALAPPDATA%\\EldenOps\\eldenops.log and setup.log
Source, credits and licence: https://github.com/reflex03/elden-ops-2
"""


def main():
    files = {
        "modengine2/config_eldenops.toml": ROOT / "package" / "config_eldenops.toml",
        "modengine2/eldenops/eldenops.dll": REL / "eldenops.dll",
        "modengine2/eldenops/EldenOps.exe": REL / "EldenOps.exe",
        "modengine2/eldenops/oat/Unlinker.exe": OAT / "Unlinker.exe",
        "modengine2/eldenops/oat/msvcp140.dll": OAT / "msvcp140.dll",
        "modengine2/eldenops/oat/vcruntime140.dll": OAT / "vcruntime140.dll",
        "modengine2/eldenops/oat/LICENSE-OpenAssetTools.txt": OAT / "LICENSE-OpenAssetTools.txt",
        "modengine2/eldenops/LICENSE.txt": ROOT / "LICENSE",
        # ModEngine2's runtime, launched from this mashup's own folder
        "modengine2/modengine2_launcher.exe": ME2 / "modengine2_launcher.exe",
        "modengine2/modengine2/bin/modengine2.dll": ME2 / "modengine2" / "bin" / "modengine2.dll",
        "modengine2/modengine2/bin/lua.dll": ME2 / "modengine2" / "bin" / "lua.dll",
        "modengine2/modengine2/crashpad/crashpad_handler.exe": ME2 / "modengine2" / "crashpad" / "crashpad_handler.exe",
        "modengine2/modengine2/crashpad/zlib1.dll": ME2 / "modengine2" / "crashpad" / "zlib1.dll",
        "modengine2/LICENSE-ModEngine2.txt": ROOT / "vendor" / "dl" / "ME2-LICENSE",
    }
    # Microsoft's x64 C++ runtime beside every ModEngine2 binary (app-local), for PCs without it
    for d in ("modengine2", "modengine2/modengine2/bin", "modengine2/modengine2/crashpad"):
        for f in ("msvcp140.dll", "msvcp140_atomic_wait.dll", "vcruntime140.dll", "vcruntime140_1.dll", "concrt140.dll"):
            files[f"{d}/{f}"] = ROOT / "vendor" / "vc64" / f
    missing = [str(p) for p in files.values() if not p.exists()]
    if missing:
        sys.exit("missing: " + ", ".join(missing))
    out = ROOT / "dist" / f"EldenOps-{VERSION}.zip"
    out.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for arc, src in files.items():
            z.write(src, arc)
        z.writestr("modengine2/eldenops/README.txt", PLAYER_README.format(v=VERSION).replace("\n", "\r\n"))
        z.writestr("modengine2/eldenops/THIRD_PARTY_NOTICES.txt", third_party_notices().replace("\n", "\r\n"))
    data = out.read_bytes()
    print(out.relative_to(ROOT), len(data), hashlib.sha256(data).hexdigest())
    with zipfile.ZipFile(out) as z:
        for i in z.infolist():
            print(f"  {i.file_size:>9}  {i.filename}", flush=True)


if __name__ == "__main__":
    main()

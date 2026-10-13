#!/usr/bin/env bash
# Downloads the third-party files the release bundles, from their official sources, and checks them.
set -euo pipefail
cd "$(dirname "$0")/.."
V=vendor
mkdir -p "$V/dl"
fetch() { # url sha256 out
  if [ ! -f "$3" ] || ! echo "$2  $3" | sha256sum -c --status; then
    curl -fsSL -o "$3" "$1"
  fi
  echo "$2  $3" | sha256sum -c
}
# OpenAssetTools v0.33.0 (GPL-3.0), Laupetin/OpenAssetTools
fetch https://github.com/Laupetin/OpenAssetTools/releases/download/v0.33.0/oat-windows.zip \
  4bf366cec03558a4b0f20bc8f197a51f4b9dad0aa3edc52b0e40fef2dd95a4f5 "$V/dl/oat-windows.zip"
# Microsoft Visual C++ 14.44 x86 runtime (app-local redistributable), from the Visual Studio 2022 channel
fetch https://download.visualstudio.microsoft.com/download/pr/45d3b8dd-bced-4b37-9974-142f748d710c/52b7fa1fd64125c5c61573a5606a48fb6e63489f89f58f8930f18ccc7e8c80a2/Microsoft.VC.14.44.17.14.CRT.Redist.X86.base.vsix \
  52b7fa1fd64125c5c61573a5606a48fb6e63489f89f58f8930f18ccc7e8c80a2 "$V/dl/vc-crt-x86.vsix"
# OpenAssetTools source license text
curl -fsSL -o "$V/dl/OAT-LICENSE" https://raw.githubusercontent.com/Laupetin/OpenAssetTools/v0.33.0/LICENSE
rm -rf "$V/oat" "$V/crt" && mkdir -p "$V/oat" "$V/crt"
unzip -q -o "$V/dl/oat-windows.zip" Unlinker.exe -d "$V/oat"
unzip -q -o "$V/dl/vc-crt-x86.vsix" 'Contents/VC/Redist/MSVC/*/x86/Microsoft.VC143.CRT/msvcp140.dll' 'Contents/VC/Redist/MSVC/*/x86/Microsoft.VC143.CRT/vcruntime140.dll' -d "$V/crt"
find "$V/crt" -name '*.dll' -exec cp {} "$V/oat/" \;
cp "$V/dl/OAT-LICENSE" "$V/oat/LICENSE-OpenAssetTools.txt"
ls -la "$V/oat"
# ModEngine2 release-2.1.0 (MIT), soulsmods/ModEngine2: the runtime files only
fetch https://github.com/soulsmods/ModEngine2/releases/download/release-2.1.0/ModEngine-2.1.0.0-win64.zip \
  8a5952453a8e3851247ed6a9dd8926ca638cf6135aa96c90298b63f15d8900ef "$V/dl/ModEngine-2.1.0.0-win64.zip"
curl -fsSL -o "$V/dl/ME2-LICENSE" https://raw.githubusercontent.com/soulsmods/ModEngine2/release-2.1.0/LICENSE-MIT 2>/dev/null \
  || curl -fsSL -o "$V/dl/ME2-LICENSE" https://raw.githubusercontent.com/soulsmods/ModEngine2/release-2.1.0/LICENSE
rm -rf "$V/me2" && mkdir -p "$V/me2"
unzip -q -o "$V/dl/ModEngine-2.1.0.0-win64.zip" -d "$V/me2"
ls "$V/me2"
# Microsoft Visual C++ 14.44 x64 runtime (app-local), for ModEngine2's launcher and crash handler
fetch https://download.visualstudio.microsoft.com/download/pr/45d3b8dd-bced-4b37-9974-142f748d710c/4aaf54db0bfc9435f7c3660e1a00237a4b556042bfeea64bde44c2e0194e6ee5/Microsoft.VC.14.44.17.14.CRT.Redist.X64.base.vsix \
  4aaf54db0bfc9435f7c3660e1a00237a4b556042bfeea64bde44c2e0194e6ee5 "$V/dl/vc-crt-x64.vsix"
rm -rf "$V/crt64" "$V/vc64" && mkdir -p "$V/crt64" "$V/vc64"
for f in msvcp140.dll msvcp140_atomic_wait.dll vcruntime140.dll vcruntime140_1.dll concrt140.dll; do
  unzip -q -o "$V/dl/vc-crt-x64.vsix" "Contents/VC/Redist/MSVC/*/x64/Microsoft.VC143.CRT/$f" -d "$V/crt64"
done
find "$V/crt64" -name '*.dll' -exec cp {} "$V/vc64/" \;
ls -la "$V/vc64"

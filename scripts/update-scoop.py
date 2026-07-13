#!/usr/bin/env python3
"""Create or update a Scoop manifest for testaruda."""
import json
import os
import sys

manifest_path = sys.argv[1]
version = sys.argv[2]
tag = sys.argv[3]
checksums_path = sys.argv[4]

sha_win = None
with open(checksums_path) as f:
    for line in f:
        parts = line.split()
        if len(parts) == 2 and "windows_amd64" in parts[1] and "adapter" not in parts[1]:
            sha_win = parts[0]
            break

if sha_win is None:
    print("ERROR: windows_amd64 checksum not found", file=sys.stderr)
    sys.exit(1)

url = f"https://github.com/charly-vibes/testaruda/releases/download/{tag}/testaruda_{version}_windows_amd64.zip"

manifest = {
    "version": version,
    "description": "Language-agnostic test selection engine",
    "homepage": "https://github.com/charly-vibes/testaruda",
    "license": "Apache-2.0",
    "url": url,
    "hash": f"sha256:{sha_win}",
    "bin": [
        "testaruda.exe",
        "testaruda-adapter-rust.exe",
        "testaruda-adapter-python.exe",
    ],
    "checkver": {
        "github": "https://github.com/charly-vibes/testaruda"
    },
    "autoupdate": {
        "url": "https://github.com/charly-vibes/testaruda/releases/download/v$version/testaruda_$version_windows_amd64.zip"
    }
}

os.makedirs(os.path.dirname(manifest_path), exist_ok=True)
with open(manifest_path, "w") as f:
    json.dump(manifest, f, indent=4)
    f.write("\n")

print(f"Wrote {manifest_path} (version {version})")
print(f"  url: {url}")
print(f"  sha256: {sha_win[:16]}...")
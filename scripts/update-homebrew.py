#!/usr/bin/env python3
"""Create or update a Homebrew formula for testaruda."""
import os
import sys

formula_path = sys.argv[1]
version = sys.argv[2]
tag = sys.argv[3]
checksums_path = sys.argv[4]

shas = {}
with open(checksums_path) as f:
    for line in f:
        parts = line.split()
        if len(parts) == 2:
            sha, name = parts
            for platform in ["darwin_arm64", "darwin_amd64", "linux_arm64", "linux_amd64"]:
                if platform in name:
                    shas.setdefault(platform, {})
                    # Multiple tarballs per platform: main + adapters
                    if "adapter-rust" in name:
                        shas[platform]["adapter-rust"] = sha
                    elif "adapter-python" in name:
                        shas[platform]["adapter-python"] = sha
                    else:
                        shas[platform]["main"] = sha

base = f"https://github.com/charly-vibes/testaruda/releases/download/{tag}"

formula = f"""\
# typed: false
# frozen_string_literal: true

class Testaruda < Formula
  desc "Language-agnostic test selection engine"
  homepage "https://github.com/charly-vibes/testaruda"
  version "{version}"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "{base}/testaruda_{version}_darwin_arm64.tar.gz"
      sha256 "{shas.get('darwin_arm64', {}).get('main', '')}"
    end
    on_intel do
      url "{base}/testaruda_{version}_darwin_amd64.tar.gz"
      sha256 "{shas.get('darwin_amd64', {}).get('main', '')}"
    end
  end

  on_linux do
    on_arm do
      if Hardware::CPU.is_64_bit?
        url "{base}/testaruda_{version}_linux_arm64.tar.gz"
        sha256 "{shas.get('linux_arm64', {}).get('main', '')}"
      end
    end
    on_intel do
      url "{base}/testaruda_{version}_linux_amd64.tar.gz"
      sha256 "{shas.get('linux_amd64', {}).get('main', '')}"
    end
  end

  def install
    bin.install "testaruda"
    bin.install "testaruda-adapter-rust"
    bin.install "testaruda-adapter-python"
  end

  test do
    system "\#{{bin}}/testaruda", "--version"
  end
end
"""

os.makedirs(os.path.dirname(formula_path), exist_ok=True)
with open(formula_path, "w") as f:
    f.write(formula)

print(f"Wrote {formula_path} (version {version})")
for p, shas_for_p in shas.items():
    for k, s in shas_for_p.items():
        print(f"  {p}/{k}: {s[:16]}...")
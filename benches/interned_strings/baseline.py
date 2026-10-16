#!/usr/bin/env python3
"""Build frozen Rust comparison executables from a committed local revision."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def run(args, **kwargs):
    return subprocess.run([str(a) for a in args], cwd=ROOT, check=True, text=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("revision")
    parser.add_argument("--output", type=Path, default=ROOT / "target/interned-strings/baseline")
    args = parser.parse_args()
    revision = run(["git", "rev-parse", "--verify", args.revision + "^{commit}"], capture_output=True).stdout.strip()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    source = output / "source"
    if source.exists():
        raise RuntimeError(f"Use a fresh output directory; {source} already exists")
    source.mkdir()
    archive = output / "source.tar"
    run(["git", "archive", "--format=tar", "--output", archive, revision])
    run(["tar", "-xf", archive, "-C", source])
    target = output / "build"
    env = os.environ.copy()
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    env["RUSTFLAGS"] = "-C target-cpu=native"
    command = ["cargo", "build", "--release", "--locked", "--no-default-features",
               "--manifest-path", source / "Cargo.toml", "--target-dir", target,
               "--example", "interned_strings", "--example", "interned_strings_memory"]
    run(command, env=env)
    names = ["interned_strings", "interned_strings_memory"]
    for name in names:
        shutil.copy2(target / "release/examples" / name, output / name)
    paths = [source / "Cargo.toml", source / "Cargo.lock",
             *sorted((source / "src").rglob("*.rs")),
             *sorted((source / "benches/interned_strings").glob("*.rs"))]
    report = {
        "revision": revision,
        "rustc": run(["rustc", "-Vv"], capture_output=True).stdout.strip(),
        "rust_flags": "release opt-level=3; lto=thin; codegen-units=1; target-cpu=native; no features; --locked",
        "build_command": [str(a) for a in command],
        "source_sha256": {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
        "binary_sha256": {name: hashlib.sha256((output / name).read_bytes()).hexdigest() for name in names},
    }
    (output / "baseline.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"Baseline {revision} saved to {output}")


if __name__ == "__main__":
    main()

"""Package explicitly unsigned beta builds from a strict public-file allowlist."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import tomllib
import zipfile


def revision(root):
    return subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--product", choices=("cli", "desktop", "server"), required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    if "-beta." not in version:
        parser.error("This tool only packages explicitly versioned beta previews")
    system = platform.system().lower()
    if system not in ("windows", "linux"):
        parser.error("Only Windows and Linux are supported")
    if platform.machine().lower() not in ("amd64", "x86_64"):
        parser.error("Only x86_64 beta binaries have been validated")
    if args.product == "desktop" and system != "windows":
        parser.error("The desktop is Windows-only")
    suffix = ".exe" if system == "windows" else ""
    stem = {"cli": "fatrocu-cli", "desktop": "fatrocu-desktop",
            "server": "fatrocu-server"}[args.product]
    binary = "fatrocu" if args.product == "desktop" else stem
    files = [(root / "target/release" / (binary + suffix), binary + suffix),
             (root / "README.md", "README.md"),
             (root / "LICENSE", "LICENSE"),
             (root / f"docs/releases/{version}.md", "BETA.md")]
    build = {"version": version, "product": args.product, "platform": system,
             "architecture": "x86_64", "commit": revision(root),
             "prerelease": True, "signed": False,
             "extraction_benchmark": "NOT RUN (dataset pending)"}
    if args.product == "cli" and system == "windows":
        files.append((root / "target/release/fatrocu-updater.exe", "fatrocu-updater.exe"))
    if args.product == "desktop":
        files.extend([
            (root.parent / "fatrocu-cli/target/release/fatrocu-updater.exe",
             "fatrocu-updater.exe"),
            (root.parent / "fatrocu-server/target/release/fatrocu-server.exe",
             "fatrocu-server.exe"),
        ])
    for dependency in ("core", "server"):
        pin = root / f"{dependency}-revision.txt"
        if pin.exists():
            dependency_root = root.parent / ("fatrocu-cli" if dependency == "core"
                                             else "fatrocu-server")
            expected = pin.read_text().strip()
            actual = revision(dependency_root)
            if actual != expected:
                parser.error(f"{dependency} checkout does not match its pinned revision")
            build[f"{dependency}_commit"] = actual
    for source, _ in files:
        if not source.is_file() or source.is_symlink():
            parser.error(f"Missing or unsafe package input: {source}")
    if args.product != "desktop":
        executable = files[0][0]
    else:
        executable = root.parent / "fatrocu-server/target/release/fatrocu-server.exe"
    observed = subprocess.check_output([str(executable), "--version"],
                                       text=True, timeout=20).strip()
    if observed.split()[-1] != version:
        parser.error("Executable version does not match the beta manifest")
    subprocess.run([str(executable), "--help"], check=True,
                   stdout=subprocess.DEVNULL, timeout=20)
    args.output.mkdir(parents=True, exist_ok=True)
    archive = args.output / f"{stem}-{version}-{system}-x86_64-unsigned.zip"
    with zipfile.ZipFile(archive, "x", zipfile.ZIP_DEFLATED) as package:
        for source, name in files:
            package.write(source, name)
        package.writestr("BUILD.json", json.dumps(build, indent=2) + "\n")
    with zipfile.ZipFile(archive) as package:
        if package.testzip() is not None:
            raise RuntimeError("ZIP integrity check failed")
        if sorted(package.namelist()) != sorted([name for _, name in files] + ["BUILD.json"]):
            raise RuntimeError("Unexpected archive content")
    digest = hashlib.file_digest(archive.open("rb"), "sha256").hexdigest()
    checksum = args.output / f"{archive.name}.sha256"
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(f"{archive.name}: SHA-256 {digest}; unsigned beta; {observed}")


if __name__ == "__main__":
    main()

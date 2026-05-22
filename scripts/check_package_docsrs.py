#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import pathlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
CRATES = [
    "mdwright-latex",
    "mdwright-math",
    "mdwright-mathrender",
    "mdwright-document",
    "mdwright-format",
    "mdwright-lint",
    "mdwright-config",
    "mdwright-lsp",
    "mdwright",
]


def fail(message: str) -> None:
    print(f"package check: {message}", file=sys.stderr)
    raise SystemExit(1)


def run(command: list[str], *, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
        check=True,
    )


def read_toml(path: pathlib.Path) -> dict:
    with path.open("rb") as fh:
        return tomllib.load(fh)


def workspace_version() -> str:
    root_manifest = read_toml(ROOT / "Cargo.toml")
    return root_manifest["workspace"]["package"]["version"]


def package_crates(allow_dirty: bool) -> None:
    command = [
        "cargo",
        "package",
        "--workspace",
        "--exclude",
        "xtask",
        "--exclude",
        "mdwright-extra-example",
        "--no-verify",
    ]
    if allow_dirty:
        command.append("--allow-dirty")
    result = run(command, capture=True)
    sys.stdout.write(result.stdout)
    sys.stderr.write(result.stderr)


def package_contents(version: str) -> None:
    required = {
        "mdwright-latex": ["Cargo.toml", "README.md", "src/lib.rs", "src/parser.rs"],
        "mdwright-math": ["Cargo.toml", "README.md", "src/lib.rs", "src/scan.rs"],
        "mdwright-mathrender": ["Cargo.toml", "README.md", "src/lib.rs", "src/check.rs"],
        "mdwright-document": ["Cargo.toml", "README.md", "src/lib.rs", "src/ir.rs"],
        "mdwright-format": ["Cargo.toml", "README.md", "src/lib.rs", "src/format/mod.rs"],
        "mdwright-lint": ["Cargo.toml", "README.md", "src/lib.rs", "src/stdlib.rs"],
        "mdwright-config": ["Cargo.toml", "README.md", "src/lib.rs", "src/config.rs"],
        "mdwright-lsp": ["Cargo.toml", "README.md", "src/lib.rs", "src/lsp.rs"],
        "mdwright": ["Cargo.toml", "README.md", "src/lib.rs", "src/bin/mdwright.rs"],
    }

    for crate in CRATES:
        package = ROOT / "target" / "package" / f"{crate}-{version}.crate"
        if not package.is_file():
            fail(f"missing packaged tarball {package}")
        with tarfile.open(package) as tar:
            names = set(tar.getnames())
        prefix = f"{crate}-{version}/"
        for rel in required[crate]:
            if prefix + rel not in names:
                fail(f"{crate} package is missing {rel}")


def docs_rs_tarball_simulation(version: str) -> None:
    with tempfile.TemporaryDirectory(prefix="mdwright-package-docsrs-") as tmp:
        workspace = pathlib.Path(tmp) / "workspace"
        workspace.mkdir()

        for crate in CRATES:
            package = ROOT / "target" / "package" / f"{crate}-{version}.crate"
            with tarfile.open(package) as tar:
                tar.extractall(workspace)

        members = ",\n  ".join(f'"{crate}-{version}"' for crate in CRATES)
        patches = "\n".join(f'{crate} = {{ path = "{crate}-{version}" }}' for crate in CRATES)
        (workspace / "Cargo.toml").write_text(
            f"""[workspace]
resolver = "3"
members = [
  {members},
]

[patch.crates-io]
{patches}
""",
            encoding="utf-8",
        )

        cargo_path = shutil.which("cargo")
        if not cargo_path:
            fail("could not locate cargo for packaged docs.rs simulation")
        cargo = pathlib.Path(cargo_path)
        safe_path = os.pathsep.join([str(cargo.parent), "/usr/bin", "/bin"])
        env = {
            "CARGO_HOME": os.environ.get("CARGO_HOME", str(pathlib.Path.home() / ".cargo")),
            "RUSTUP_HOME": os.environ.get("RUSTUP_HOME", str(pathlib.Path.home() / ".rustup")),
            "HOME": os.environ.get("HOME", str(pathlib.Path.home())),
            "PATH": safe_path,
            "DOCS_RS": "1",
            "RUSTDOCFLAGS": "-D warnings",
            "CARGO_TERM_COLOR": os.environ.get("CARGO_TERM_COLOR", "always"),
        }

        for crate in CRATES:
            subprocess.run(
                [
                    str(cargo),
                    "doc",
                    "--manifest-path",
                    str(workspace / "Cargo.toml"),
                    "--no-deps",
                    "-p",
                    crate,
                ],
                env=env,
                check=True,
            )


def release_docs_exist() -> None:
    for rel in [
        "docs/api-review/mdwright-public.txt",
        "docs/api-review/mdwright-latex-public.txt",
        "docs/src/reference/release-evidence.md",
        "docs/src/reference/crates-io-release.md",
    ]:
        if not (ROOT / rel).is_file():
            fail(f"release documentation is missing {rel}")


def main() -> None:
    parser = argparse.ArgumentParser(description="Check mdwright package contents and docs.rs-mode docs from tarballs.")
    parser.add_argument(
        "--allow-dirty",
        action="store_true",
        help="pass --allow-dirty to cargo package; intended for local verification before committing",
    )
    args = parser.parse_args()

    version = workspace_version()
    package_crates(args.allow_dirty)
    package_contents(version)
    release_docs_exist()
    docs_rs_tarball_simulation(version)
    print("package check: docs.rs tarball simulation passed")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Reject new project Markdown files outside the maintained guides."""
from pathlib import Path, PurePosixPath
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PROJECT_GUIDES = {
    "AGENTS.md", "README.md", "WINDOWS.md", "UPDATES.md", "LICENSES.md",
    "DEVELOPMENT.md", "RELEASE_NOTES.md", "MATH_SOLVER_DESIGN.md", "OCR_FIRST_USE.md",
}


def main():
    # Include force-added files: .gitignore alone cannot protect CI against them.
    output = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=ROOT,
    )
    unexpected = []
    for raw in set(output.split(b"\0")) - {b""}:
        name = raw.decode("utf-8", errors="surrogateescape")
        path = PurePosixPath(name)
        if path.suffix.lower() != ".md":
            continue
        file = ROOT / name
        if not file.exists() and not file.is_symlink():
            continue  # A tracked document deleted in the working tree.
        if name in PROJECT_GUIDES or path.parts[0] in {"third_party", "vendor"}:
            continue
        unexpected.append(name)
    if unexpected:
        print("Unexpected Markdown files:\n" + "\n".join(sorted(unexpected)))
        print("Update an existing guide; put reports and generated evidence in artifacts/.")
        return 1
    print("Documentation allowlist passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

"""Shared, portable desktop and offline math payload for Linux packages."""
import json
from pathlib import Path
import shutil
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
APP_ID = "io.github.folio.Notes"


def version():
    return tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]


def copy(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)


def stage_payload(prefix, binary, math_python):
    """Never copy the development venv, models, databases or absolute pack paths."""
    actual = subprocess.check_output([str(binary.resolve()), '--version'], text=True).strip()
    if actual != f'Folio {version()}':
        raise ValueError(f'Binary version does not match package: {actual!r}')
    copy(binary, prefix / "bin/folio-native")
    copy(ROOT / "packaging/folio-launcher", prefix / "bin/folio")
    copy(ROOT / "packaging/folio-math-python", prefix / "bin/math-python")
    for name in ("folio", "folio-native", "math-python"):
        (prefix / "bin" / name).chmod(0o755)
    math_pack = prefix / "math-solver"
    for name in ("math-solver-worker.py", "math_parser.py"):
        copy(ROOT / "scripts" / name, math_pack / name)
    # Both packages are pure Python and use the distro/Flatpak Python.
    probe = """
import importlib.metadata, json
result = {}
for name in ('sympy', 'mpmath'):
    dist = importlib.metadata.distribution(name)
    result[name] = {'version': dist.version,
                    'package': str(dist.locate_file(name)),
                    'metadata': str(dist._path)}
print(json.dumps(result))
"""
    distributions = json.loads(subprocess.check_output([str(math_python), "-c", probe], text=True))
    for name, expected in (("sympy", "1.14.0"), ("mpmath", "1.3.0")):
        dist = distributions[name]
        if dist["version"] != expected:
            raise ValueError(f"Packaging needs {name}=={expected}, got {dist['version']}")
        for key in ("package", "metadata"):
            source = Path(dist[key])
            shutil.copytree(source, math_pack / "site-packages" / source.name,
                            ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "*.pyo"))
    config = {"python": "../bin/math-python", "worker": "math-solver-worker.py",
              "timeout_seconds": 8, "sympy": "1.14.0", "mpmath": "1.3.0",
              "engine": "folio-math-1"}
    (math_pack / "pack.json").write_text(json.dumps(config, indent=2) + "\n")


def stage_metadata(prefix):
    for source, destination in (
        (ROOT / f"packaging/{APP_ID}.desktop", f"share/applications/{APP_ID}.desktop"),
        (ROOT / f"packaging/{APP_ID}.svg", f"share/icons/hicolor/scalable/apps/{APP_ID}.svg"),
        (ROOT / f"packaging/{APP_ID}.metainfo.xml", f"share/metainfo/{APP_ID}.metainfo.xml"),
        (ROOT / "LICENSE", "share/doc/folio/LICENSE"),
        (ROOT / "LICENSES.md", "share/doc/folio/LICENSES.md"),
    ):
        copy(source, prefix / destination)
    shutil.copytree(ROOT / "third_party/licenses", prefix / "share/doc/folio/third_party")
    shutil.copytree(ROOT / "third_party/ocr", prefix / "share/doc/folio/ocr")

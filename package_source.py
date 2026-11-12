"""Package only the files needed to inspect, build, and modify Cat Home."""

import argparse
from pathlib import Path
import re
from tempfile import NamedTemporaryFile
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo


ROOT = Path(__file__).resolve().parent
DEFAULT_ARCHIVE = (
    ROOT.parents[1] / "outputs" / "猫咪小屋" / "猫咪小屋源码.zip"
    if ROOT.parent.name == "work"
    else ROOT.parent / "猫咪小屋源码.zip"
)
FILES = (
    ".gitignore",
    "Cargo.toml",
    "Cargo.lock",
    "README.md",
    "LICENSE",
    "CREDITS.md",
    "package_source.py",
    "src/main.rs",
    "src/activities.rs",
    "src/renderer.rs",
    "src/simulation.rs",
    "src/world.rs",
    "design/animals.tsv",
    "design/interaction.tsv",
    "design/visitors-yard.tsv",
    "design/export_table.py",
    "bake_scene.py",
    "bake_world.py",
    "sources/cat.json",
    "sources/cat.png",
    "sources/tiny-town.zip",
    "assets/world/black_cat.bgra",
    "assets/world/builds.bgra",
    "assets/world/cat_eat.bgra",
    "assets/world/cat_idle.bgra",
    "assets/world/cat_sleep.bgra",
    "assets/world/cat_treat.bgra",
    "assets/world/cat_walk.bgra",
    "assets/world/construction.bgra",
    "assets/world/grey_cat.bgra",
    "assets/world/keeper.bgra",
    "assets/world/preview.png",
    "assets/world/scenes.bgra",
    "assets/world/sparrow.bgra",
)
INCLUDE = re.compile(r'include_(?:bytes|str)!\(\s*"([^"]+)"\s*\)')
MODULE = re.compile(r"^mod (\w+);", re.MULTILINE)


def source_files():
    paths = {}
    for name in FILES:
        path = ROOT / name
        resolved = path.resolve(strict=True)
        if resolved != path or not resolved.is_file():
            raise ValueError(f"Expected a regular project file: {path}")
        paths[name] = path
    for path in paths.values():
        if path.suffix != ".rs":
            continue
        code = path.read_text(encoding="utf-8")
        dependencies = INCLUDE.findall(code)
        dependencies.extend(f"{name}.rs" for name in MODULE.findall(code))
        for dependency in dependencies:
            resolved = (path.parent / dependency).resolve()
            relative = resolved.relative_to(ROOT).as_posix()
            if relative not in paths:
                raise ValueError(f"{path.name} requires {relative} in the archive")
    return paths


def write_archive(paths, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    with NamedTemporaryFile(dir=destination.parent, suffix=".zip", delete=False) as file:
        temporary = Path(file.name)
    try:
        with ZipFile(temporary, "w") as archive:
            for name, path in paths.items():
                entry = ZipInfo(f"cat-home/{name}")
                entry.compress_type = ZIP_DEFLATED
                archive.writestr(entry, path.read_bytes())
        with ZipFile(temporary) as archive:
            expected = [f"cat-home/{name}" for name in paths]
            if archive.namelist() != expected or archive.testzip() is not None:
                raise ValueError("Source archive failed verification")
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify the file list without writing")
    parser.add_argument("--output", type=Path, default=DEFAULT_ARCHIVE)
    options = parser.parse_args()
    paths = source_files()
    if not options.check:
        write_archive(paths, options.output)
        print(f"Source archive: {options.output}")
    print(f"Verified {len(paths)} source files")


if __name__ == "__main__":
    main()

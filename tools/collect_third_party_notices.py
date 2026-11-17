"""Collect the notices carried by the Windows release from locked inputs."""

import argparse
import json
import re
import subprocess
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "THIRD_PARTY_NOTICES.txt"
RUST_RELEASE = "1.97.1"
WINDOWS_TARGETS = ("x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc")


class NoticeText(HTMLParser):
    def __init__(self):
        super().__init__()
        self.parts = []
        self.body = False
        self.pre = False

    def handle_starttag(self, tag, attrs):
        if tag == "body":
            self.body = True
        if self.body and tag in {"p", "h1", "h2", "h3", "h4", "pre", "li", "br"}:
            self.parts.append("\n")
        if tag == "pre":
            self.pre = True

    def handle_endtag(self, tag):
        if self.body and tag in {"p", "h1", "h2", "h3", "h4", "pre", "li", "div"}:
            self.parts.append("\n")
        if tag == "body":
            self.body = False
        if tag == "pre":
            self.pre = False

    def handle_data(self, data):
        if self.body:
            self.parts.append(data if self.pre else re.sub(r"\s+", " ", data))

    def text(self):
        return re.sub(r"\n{3,}", "\n\n", "\n".join(
            line.strip() for line in "".join(self.parts).splitlines()
        )).strip()


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, encoding="utf-8")


def read(path):
    return "\n".join(line.rstrip() for line in path.read_text(
        encoding="utf-8-sig").strip().splitlines())


def dependency_notices(toolchain, offline):
    packages = {}
    for target in WINDOWS_TARGETS:
        args = ["cargo", *toolchain, "metadata", "--locked", "--format-version", "1",
                "--filter-platform", target]
        if offline:
            args.append("--offline")
        metadata = json.loads(command(*args))
        root = metadata["resolve"]["root"]
        for package in metadata["packages"]:
            if package["id"] != root:
                packages[package["name"], package["version"]] = package

    # Keep the upstream license variants, including mandatory notices for dual-licensed crates.
    # A crate may use different spelling or case for its license files.
    for (name, version), package in sorted(packages.items()):
        directory = Path(package["manifest_path"]).parent
        paths = sorted(path for path in directory.iterdir() if path.is_file() and
                       re.match(r"^(license|licence|copying|notice|unlicense)([._-]|$)",
                                path.name, re.IGNORECASE))
        if not package["license"] or not paths:
            raise SystemExit(f"Missing verified license for {name} {version}")
        yield (
            f"{name} {version}  ·  {package['license']}\n"
            f"https://crates.io/crates/{name}/{version}",
            [(path.name, read(path)) for path in paths],
        )


def render(toolchain, offline):
    rust = command("rustc", *toolchain, "--version").split()
    if rust[1] != RUST_RELEASE:
        raise SystemExit(f"Refresh compiler notices with Rust {RUST_RELEASE} (found {rust[1]}).")
    docs = Path(command("rustc", *toolchain, "--print", "sysroot").strip()) / "share/doc/rust"
    upstream = NoticeText()
    upstream.feed(read(docs / "COPYRIGHT-library.html"))
    sections = ["""柳溪村 · 第三方许可与鸣谢

项目代码 · MIT
Windows 透明窗口、绘制和托盘代码改自 Yannick Herrero 的 Nekodachi。
https://github.com/YannickHerrero/nekodachi/tree/41745bca9db334a911669f651fb5803a5d72fd6f
Microsoft 的 Win32 API 绑定由 windows-sys 提供。依赖的具体版本及原始许可在下方。

素材 · CC0 1.0
LuizMelo 的 Pet Cats Pack 橘猫经 Todak 的 Ginger CC0 图集取得并改编。
https://luizmelo.itch.io/pet-cat-pack
https://github.com/euuuuuuan/todak-public/blob/main/CREDITS.md
Kenney 的 Tiny Town 用于早期庭院素材。
https://kenney.nl/assets/tiny-town
https://creativecommons.org/publicdomain/zero/1.0/
新增柳溪村素材及图标使用图像生成工具和项目绘图脚本制作。详细出处见 CREDITS.md。

字体 · Noto Sans SC 2.004 · SIL Open Font License 1.1
村民和访客的姓名牌、小红书七图使用该字体制作。交付内容为固定文字图片。
https://github.com/notofonts/noto-cjk/releases/tag/Sans2.004
下方附字体版权及完整许可。

官方 Windows 发布版使用 Visual Studio C++ 可再发行静态运行库。
https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution

项目与完整素材说明
https://github.com/forguse/liuxi-village/blob/main/CREDITS.md
""", read(ROOT / "LICENSE"), read(ROOT / "assets/fonts/OFL-NotoSansSC.txt")]
    repeated = {}
    for title, notices in dependency_notices(toolchain, offline):
        sections.append(f"{'=' * 64}\n{title}")
        for name, notice in notices:
            if notice in repeated:
                sections.append(f"{name}\n全文见 {repeated[notice]}。")
                continue
            repeated[notice] = f"{title.split('  ·  ')[0]} 的 {name}"
            sections.append(f"{name}\n\n{notice}")
    sections.extend([
        f"{'=' * 64}\nRust {RUST_RELEASE} · Standard Library",
        upstream.text(),
    ])
    for path in sorted((docs / "licenses").glob("*.txt")):
        notice = read(path)
        if notice in repeated:
            sections.append(f"{path.name}\n全文见 {repeated[notice]}。")
        else:
            sections.append(f"{path.name}\n\n{notice}")
    return "\n\n".join(sections).strip() + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain", help="Rustup toolchain name")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    toolchain = ["+" + args.toolchain] if args.toolchain else []
    data = render(toolchain, args.offline)
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text(encoding="utf-8") != data:
            raise SystemExit("THIRD_PARTY_NOTICES.txt is stale. Regenerate before releasing.")
    else:
        OUTPUT.write_text(data, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()

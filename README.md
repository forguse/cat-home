# 猫咪小屋 · 柳溪村

**挂在 Windows 桌面一角的像素村庄。** 小禾和阿榆自己种菜、做饭、钓鱼、赶集，橘猫会在院子里打盹、巡逻和躲雨。你可以照常做自己的事，偶尔看一眼村里过得怎么样。

**[下载 Windows 版 EXE](https://github.com/forguse/cat-home/releases/latest/download/cat-home-windows-x64.exe)** · [版本说明与校验值](https://github.com/forguse/cat-home/releases/latest)

免费开源 · Windows x64 · 单文件约 29 MB（v1.1.9）

![小禾、阿榆与橘猫在柳溪村小院的实机画面](docs/media/cat-home-village.png)

## 开始使用

1. 下载 `cat-home-windows-x64.exe`，放进一个你有写入权限的普通文件夹，再双击打开。
2. 左键拖动小屋到合适的位置，拖右下角调整视野大小。
3. 右键打开「观察手记」，查看角色在做什么、村里有什么资源，以及最近留下的足迹和观察记录。

| 想做什么 | 操作 |
| --- | --- |
| 换个视角 | 右键 → 镜头，选择自动观察、跟随主角或回到庭院。按住 Shift 拖动可以手动取景。 |
| 加一个活动 | 右键 → 插件 → 安装插件，选择 `.catpack` 文件。仓库有[篮球示例包](content-packs/basketball)，活动需要已开放场地。 |
| 登录时打开、隐藏或退出 | 右键小屋或任务栏通知区域的图标。 |

## 村里会发生什么

- **小禾和阿榆有自己的作息。** 他们会取工具、料理庄稼、搬运收成，也会同桌吃饭、在忙完后歇一会儿。橘猫会跟着院子里的生活找事做。
- **劳动会改变村庄。** 砍下来的木材要实际搬回仓，收来的菜会入库、做成饭或拿去交换。清出的地可以继续开垦，房子要经历备料、施工和启用。
- **村庄会慢慢长大。** 新档从两户人的小院与几块菜地开始。随着真实日历推移，市场、街道和更多住所逐批有机会开工，还要等人手与材料到位。
- **昼夜、天气和季节会改变日常。** 重新打开时，程序会分批补算关掉期间的生活。调整窗口大小只改变可见范围，视野外的生活仍照常模拟。

v1.1.9 共接入 **100 类基础行为、294 条条件变体**。新增场景包括实际让路并留下足迹、搬运归仓、雨停后续走，以及秋天清晨燕子和麻雀一同落在栏杆上。[查看本版清单](docs/LEGACY_VARIANTS_1_1_9.md)

## 更新、迁移和卸载

- **更新**　先在菜单中退出，再用新版替换同目录的 EXE。进度在同目录的 `home-data-v2/world-v4.json`。版本迁移会保留升级前的存档副本。
- **搬到别的文件夹**　连同 `home-data-v2` 和 `cat-home-position.txt` 一起移动。
- **卸载**　取消「登录 Windows 时启动」，退出后删除 EXE。保留 `home-data-v2`，以后可继续同一个村庄。

## 构建与项目文档

本项目使用 Rust（2024 edition）和原生 Win32 窗口。GNU 工具链需要 `windres`，MSVC 工具链可使用 Windows SDK 的 `rc`。

```powershell
cargo test --locked
cargo build --locked --release
```

构建结果位于 `target/release/cat-home.exe`。常规构建使用仓库已生成的地图和图集。修改地图后运行 `python tools/compile_map.py`，并提交对应生成文件。

- [真实时间与城镇发展](docs/REAL_TIME_TOWN.md) · [地图与地貌资源](docs/WORKING_LANDSCAPE.md)
- [插件开发](docs/PLUGINS.md) · [验证记录](docs/VALIDATION.md) · [行为目录与逐条审查](docs/current-behavior-audit.tsv)
- [反馈问题](https://github.com/forguse/cat-home/issues)

代码采用 [MIT 许可](LICENSE)。像素素材的生成来源和第三方素材许可见 [CREDITS.md](CREDITS.md)。

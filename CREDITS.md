# 素材和代码来源

- 柳溪村的房屋、人物、动物、果树、市场正反面、生活道具和应用图标使用 OpenAI 图像生成工具按项目自定设计制作；没有从商业游戏提取素材。原始图在 `assets/premium/source/` 与 `assets/app/icon-source.png`，发布用图集由 `tools/bake_premium_assets.py`、`bake_village_art.py`、`bake_wildlife_assets.py` 进行裁切、尺寸标定和透明格式转换。项目提供的这些新增素材按 CC0 1.0 提供；生成来源与第三方来源分开记录。
- 柳溪村的地图、建筑足印、地形通行、市场工位和布景坐标是项目自定数据，存放于 `assets/maps/`。可执行程序的图标来自该项目的原创橘猫与小屋设计，来源描述在 `assets/app/icon-origin.txt`。

- Windows 透明窗口、绘制和托盘代码改自 Yannick Herrero 的 **Nekodachi**，使用 MIT 许可，版权声明见 `LICENSE`。[Nekodachi 源代码](https://github.com/YannickHerrero/nekodachi/tree/41745bca9db334a911669f651fb5803a5d72fd6f)
- Rust 的 Win32 API 绑定来自 Microsoft **windows-rs** 的 `windows-sys`，本项目按 MIT 许可使用，版权声明见 `LICENSE`。[windows-rs 的 MIT 许可](https://github.com/microsoft/windows-rs/blob/master/license-mit)
- 橘猫动画改自 LuizMelo 的 **Pet Cats Pack**，通过 Todak 的 Ginger CC0 图集取得。原图与帧坐标保存在 `sources/cat.png` 和 `sources/cat.json`。[作者素材页](https://luizmelo.itch.io/pet-cat-pack) · [Todak 图集署名](https://github.com/euuuuuuan/todak-public/blob/main/CREDITS.md#pet-atlases--what-ships-and-what-each-was-packed-from) · [图集原图](https://github.com/euuuuuuan/todak-public/blob/main/assets/atlases/pets/cat_normal_cc0_ginger.png) · [帧坐标](https://github.com/euuuuuuan/todak-public/blob/main/assets/atlases/pets/cat_normal_cc0_ginger.json)
- 早期庭院使用过 Kenney 的 **Tiny Town**，许可为 CC0 1.0。原素材包保存在 `sources/tiny-town.zip`，供早期绘制脚本使用。[作者素材页](https://kenney.nl/assets/tiny-town) · [GitHub 镜像](https://github.com/bill2432/kenney-2d-mirror)
- 新版庭院参考《星露谷物语》公开画面的斜俯视构图、乡村色彩和地块过渡，新增图形由本项目绘制。[ConcernedApe 官方媒体页](https://www.stardewvalley.net/media/)
- 项目绘制的管理员、灰猫、黑猫、麻雀、松鼠、刺猬、兔子、青蛙、扩充图集的 21 位角色、扩建建筑、施工道具、庭院布置与光照也按 CC0 1.0 提供。生成的像素图集在 `assets/world/`，绘图脚本采用 MIT 许可。[CC0 1.0 条款](https://creativecommons.org/publicdomain/zero/1.0/)

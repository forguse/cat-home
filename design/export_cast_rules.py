"""Derive the native cast's compact pose, target and trigger table from the design."""

from pathlib import Path

from export_characters import HERE, PROFILES, load_tsv, resident_rows


def match(text, *rules, default):
    return next((key for key, words in rules if any(word in text for word in words.split("|"))), default)


def trigger(when, name):
    return match(
        when,
        ("knock", "敲窗"), ("fish", "钓鱼|垂钓|钓到|有鱼|鱼碟"),
        ("treat", "玩家给零食"), ("food", "访客点心|点心盘|点心碟"),
        ("flood", "积涝|洪水|水漫"), ("storm", "风暴|阵风过后|大风吹过"),
        ("frost", "霜灾|冻伤|霜冻|重霜"), ("drought", "干旱|干裂"),
        ("damage", "受损|待修|破损|灾后|松动|修复|病苗"),
        ("nap", "午睡"), ("sleep", "入睡|猫睡|正在睡"),
        ("eat", "正餐|临近开饭|开饭铃|临近正餐|备餐|喂饭"),
        ("build", "施工班|工程缺料|施工期间|施工时|在建工程"),
        ("finished", "完工|搭好|新修好|刚修好|修建完成"),
        ("frame", "框架|地基完成|短跳板"),
        ("harvest", "收获|采收|采完|收成|成熟期结束"),
        ("budding", "将成熟|快成熟|花苞|现蕾"),
        ("mature", "成熟|长成|开花|盛开|果实"),
        ("sprout", "刚发芽|冒头|嫩苗|刚长成|萌芽"),
        ("growing", "生长期|幼苗|长密|低枝|菜苗"),
        ("seed", "播种|新物种|空花畦|空畦|翻好"),
        ("snow", "下雪|薄雪|雪后|雪盖"), ("wet", "雨后|雨停|刚下雨|水洼|湿石|潮湿|湿泥|退水"),
        ("rain", "下雨|连续雨|雨天|阵雨|小雨|暴雨|降雨"),
        ("winter", "冬季|越冬|冷天"), ("autumn", "秋季|秋天|秋夜|秋凉"),
        ("spring", "春天|春归|新一轮春季|春季"),
        ("night", "夜间|夜里|深夜|雨夜|冷夜|夜晚|夜深|黑暗"),
        ("dusk", "黄昏|薄暮|傍晚"), ("dawn", "清晨|黎明|晨间"),
        ("day", "白天|下午|午后|晴天|日照|阳光"),
        ("water", "水碗|饮水"), ("lamp", "门灯|熄灯|灯亮|照明"),
        ("expanded", "新区域|新开|新地块|拉宽|拉高|窗口改形"),
        default="rest" if "歇一会" in name else "any",
    )


def target(name, detail, gate):
    if gate in {"storm", "frost", "drought", "flood", "damage"}:
        return "damage"
    return match(
        name + " " + detail,
        ("mat", "软垫|猫垫|膝上"), ("mailbox", "信槽|便笺|信箱|信封|送信"),
        ("lamp", "灯罩|门灯|灯下|灯前|暖灯|灯位|灯影"),
        ("pond", "池边|池面|池塘|鱼塘|池沿|鱼线|水沟|水域|浅湾|蝌蚪"),
        ("water", "水碗|饮水|清水|碗边|水纹"),
        ("pantry", "食盆|饭盘|餐盘|点心盘|零食|正餐|围裙|开饭|猫粮|分餐|厨"),
        ("build", "施工|温室|框架|工位|工具棚|木匠|工架|工地|基石|花槽|木料|工料|图纸|工棚|工作台"),
        ("tree", "树干|树根|果树|树冠|苹果|坚果|树梢|树枝|树芽|树荫|矮树|枝杈|枝头"),
        ("crop", "菜苗|萝卜|南瓜|菜畦|菜行|菜篮|菜叶|收菜|收成|菜根"),
        ("flower", "花丛|花畦|花盆|花花|花朵|花苞|花瓣|花香|花籽|花坛|花束|花心"),
        ("sky", "屋脊|飞过|飞离|巢口|泥巢|飞入|翅尖|巢底|高飞|高处"),
        ("fence", "围栏|篱笆|栏杆|栅门|矮墙|界石"),
        ("porch", "台阶|门廊|门前|门边|门柱|屋檐|窗边|回屋"),
        ("path", "石径|小路|小径|路口|踏石|石路|道路"),
        ("garden", "地块|苗|播种|种子|浇水|草地|新土|院前"),
        default="path",
    )


def pose(name, detail):
    motion = match(
        name + " " + detail,
        ("nap", "入睡|打盹|午睡|蜷卧|缩成|伏低|小憩|蜷成"),
        ("hop", "飞过|飞起|跃|跳|抖翅|抬升|滑过|掠过|追捕|扑腾"),
        ("patrol", "往返|巡|来回|沿着|绕|拖|搬|折返|并行|散步|跟着|追"),
        ("tilt", "探头|侧头|歪头|观察|打量|闻|嗅|竖耳|转过耳|伸爪|张望"),
        ("bob", "撒|剪|敲|啄|整理|压|耙|清理|啃|低头|装|写|挖|拨|揉|扶|给|试|叼"),
        default="still",
    )
    prop = match(
        name + " " + detail,
        ("lamp", "灯罩|门灯|灯影|暖灯|灯位"), ("book", "手记|图纸|画板|写生|木签"),
        ("note", "便笺|信封|信箱|鸣叫|合鸣|音符|回声"),
        ("puddle", "水洼|积涝|池面|积水|水线|水位"),
        ("water", "水碗|水珠|浇|补水|清水|水纹|水点|倒水|雨珠"),
        ("snow", "薄雪|雪地|雪爪|霜冻|霜灾"), ("rain", "雨衣|雨披|雨靴|下雨|落雨"),
        ("flower", "花朵|花苞|花瓣|花束|花丛|花心|花香"),
        ("feather", "羽毛|软羽|抖羽"), ("seed", "种子|种荚|花籽|草籽|种壳|坚果"),
        ("leaf", "干叶|落叶|叶片|嫩叶|菜叶|草叶|腐叶|枝梢"),
        ("wood", "枯枝|草梗|木板|木梁|木料|木片|横梁|木桩|篱笆|木屑"),
        ("tool", "工具|锤|铲|扫|窗框|敲紧|修补|抹布|钉"),
        ("stone", "石块|石子|踏石|石头|界石|基石"),
        ("food", "盘|食物|点心|零食|餐|鱼|果子|苹果|菜篮|胡萝卜|南瓜|收成"),
        ("butterfly", "飞虫|蝴蝶|蜜蜂|虫群"), ("paw", "脚印|足印|踩|蹼印"),
        ("heart", "友好|眨眼|招呼|问好|示意"), ("bell", "铃|响动|惊响"),
        ("star", "入睡|打盹|午睡|蜷卧|影子|月光|亮点|星点"),
        ("crumb", "碎屑|颗粒|碎籽"), ("mist", "阴影|雾|阴凉|湿痕"),
        default="spark",
    )
    return motion, prop


def main():
    profiles = [line.split("|") for line in PROFILES.splitlines()]
    rows = resident_rows()
    for filename in ("character-humans.tsv", "character-animals-a.tsv", "character-animals-b.tsv"):
        rows += load_tsv(filename, 8)
    shared = load_tsv("character-shared.tsv", 6)
    lines = ["\t".join(("@" + code, label, tags)) for code, label, tags, _ in profiles]
    for row in rows + shared:
        if len(row) == 8:
            code, role, _, _, name, when, detail, _ = row
        else:
            code, _, name, when, detail, _ = row
            role = ""
        partner = next(
            (other for other, label, _, _ in profiles if other != role and label.split("（")[0] in when),
            "-",
        )
        gate = trigger(when, name)
        site = target(name, detail, gate)
        motion, prop = pose(name, detail)
        lines.append("\t".join((code, gate, site, motion, prop, partner)))
    assert len(lines) == 30 + 424
    (HERE / "cast-rules.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()

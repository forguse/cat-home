"""Make an offline role and behavior design table."""
from collections import Counter
from csv import reader
from html import escape as e
from pathlib import Path
import sys


HERE = Path(__file__).resolve().parent
PROFILES = """
H01|小园丁（管理员）|human ground plant build social wet|常住。戴草帽的小女孩，照料橘猫、种植、修建和记手记
H02|园艺师|human visitor ground plant social|病苗、新地块或换季时到访，负责育苗和诊断
H03|木匠|human visitor ground build social|有待料工位时在白天来，带构件或修工具
H04|巡夜人|human visitor ground night social|夜间灯位异常或风停后巡院
H05|气象观察员|human visitor ground wet social|天气将变化或灾害结束后到访
H06|厨师|human visitor ground social|午饭前、收获后或有鱼时到访
H07|送信员|human visitor ground social|白天下午在院门交接包裹和便笺
H08|修水工|human visitor ground build wet social|池塘完工、雨后或有积涝时到访
H09|画师|human visitor ground social|光照好的早晨或黄昏到院中写生
H10|种子商|human visitor ground plant social|隔几个世界日来一次，交换少量种子
A01|橘猫|cat ground night social|常住。白天活动，午饭后在室外小睡，夜里回屋
A02|灰猫|cat visitor ground social|白天沿石径来，喜欢在暖处坐一会儿
A03|黑猫|cat visitor ground night social|傍晚从门廊进院，熟悉暖灯和暗处
A04|麻雀|visitor flight small social|白天在栏杆、石径和浅水边落脚
A05|松鼠|visitor ground small plant social|干燥白天出现，留意树枝和种子
A06|刺猬|visitor ground small night|傍晚或夜里沿篱笆边找食
A07|兔子|visitor ground social plant|清晨或白天来到菜畦外围
A08|青蛙|visitor ground small wet|下雨或雨后，主要在水洼和鱼塘附近
A09|燕子|visitor flight social|温暖白天掠过屋檐和院口
A10|小鸭|visitor ground wet social|湿天或水边有空位时进入庭院
A11|蜗牛|visitor ground small wet plant|在雨后沿花盆和湿石慢慢移动
A12|瓢虫|visitor flight small plant|晴暖时停在花叶和菜苗上
A13|萤火虫|visitor flight small night|暖夜或薄暮在较暗的花丛附近发光
A14|蜜蜂|visitor flight small plant social|白天有花开放时顺着花丛觅食
A15|乌鸦|visitor flight social|白天在高处巡视，偶尔叼来小物
A16|山雀|visitor flight small social plant|冷天或清晨到树枝、篱笆和果枝
A17|蝴蝶|visitor flight small plant|晴暖时从一朵花慢慢飞向另一朵
A18|蜥蜴|visitor ground small|晴天下午在热石和墙根之间往返
A19|田鼠|visitor ground small night plant|薄暮在矮墙下和采收后的畦边活动
A20|白鸽|visitor flight social|白天在院口落地，沿开阔石径散步
""".strip()


def load_tsv(name, fields):
    with (HERE / name).open(encoding="utf-8-sig", newline="") as source:
        rows = [row for row in reader(source, delimiter="\t") if row]
    assert all(len(row) == fields and all(row) for row in rows), name
    return rows


def resident_rows():
    rows = []
    for line in (HERE / "character-residents.txt").read_text(encoding="utf-8").splitlines():
        if line.startswith("@"):
            role, index = line[1:], 0
        elif line.strip():
            index += 1
            name = "小园丁（管理员）" if role == "H01" else "橘猫"
            category = "人类" if role == "H01" else "动物"
            rows.append([f"{role}-{index:02d}", role, name, category, *(part.strip() for part in line.split("|"))])
    return rows


def row_html(row, data_role, kind):
    fields = "".join(f"<td>{e(str(field))}</td>" for field in row)
    return f'<tr data-role="{e(data_role)}" data-kind="{kind}">{fields}</tr>'


def main():
    profiles = {role: (name, tags.split(), timing) for role, name, tags, timing in
                (line.split("|") for line in PROFILES.splitlines())}
    shared = load_tsv("character-shared.tsv", 6)
    exclusive = resident_rows()
    for name in ("character-humans.tsv", "character-animals-a.tsv", "character-animals-b.tsv"):
        exclusive += load_tsv(name, 8)
    counts = Counter(row[1] for row in exclusive)
    assert len(profiles) == 30 and set(profiles) == set(counts)
    assert all(counts[role] == (60 if role in {"H01", "A01"} else 10) for role in profiles), counts
    assert len(exclusive) == 400 and len(shared) == 24
    assert len({row[0] for row in exclusive + shared}) == 424
    tags = {tag for _, role_tags, _ in profiles.values() for tag in role_tags}
    applies = {}
    for code, eligibility, *_ in shared:
        requirements = set(eligibility.split("|"))
        assert requirements == {"all"} or requirements <= tags, (code, requirements)
        applies[code] = [role for role, (_, role_tags, _) in profiles.items()
                         if requirements == {"all"} or requirements.intersection(role_tags)]
        assert len(applies[code]) >= 2, code
    roster = []
    options = []
    for role, (name, _, timing) in profiles.items():
        category = "人类" if role.startswith("H") else "动物"
        common = sum(role in roles for roles in applies.values())
        options.append(f'<option value="{role}">{e(name)}</option>')
        roster.append(f'<tr><td><button type="button" data-pick="{role}">{e(name)}</button></td>'
                      f"<td>{category}</td><td>{e(timing)}</td><td>{counts[role]}</td><td>{common}</td></tr>")
    rows = []
    for code, role, _, _, action, when, visible, effect in sorted(exclusive):
        rows.append(row_html([code, profiles[role][0], "专属", action, when, visible, effect], role, "exclusive"))
    for code, _, action, when, visible, effect in shared:
        eligible = applies[code]
        names = "全部角色" if len(eligible) == 30 else "、".join(profiles[role][0] for role in eligible)
        rows.append(row_html([code, names, "共享", action, when, visible, effect], ",".join(eligible), "shared"))
    html = """<!doctype html><html lang="zh-CN"><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>猫咪小屋 · 角色与行为设计</title>
<style>
:root{color-scheme:light dark;font-family:system-ui,"Microsoft YaHei",sans-serif;line-height:1.55;background:#171821;color:#eee6de}
body{margin:0}main{max-width:1320px;margin:auto;padding:32px 24px 60px}h1{font-size:clamp(28px,4vw,44px);margin:0 0 .3em;color:#f9d6a6}
h2{margin-top:32px}p{margin:.55em 0;color:#e1d5d4}a{color:#b6dacb}.scroll{overflow:auto;border:1px solid #4a4052;border-radius:10px}
table{width:100%;border-collapse:collapse;min-width:810px;font-size:14px}th,td{padding:10px 12px;border-bottom:1px solid #43394d;text-align:left;vertical-align:top}
th{background:#392e41;color:#f6dfc0}tr:nth-child(even){background:#242230}td:first-child{white-space:nowrap;color:#e7bd96}
#details td:nth-child(2){width:14%}#details td:nth-child(3){white-space:nowrap}#details td:nth-child(4){width:16%;color:#f6d5aa}
.filters{display:flex;flex-wrap:wrap;align-items:center;gap:8px;padding:10px 0;margin-bottom:8px;position:sticky;top:0;background:#171821}
input,select,button{font:inherit;color:inherit;background:#2c2636;border:1px solid #675470;border-radius:7px;padding:6px 10px}
input{width:min(100%,23rem)}button{cursor:pointer}button:hover{background:#4a3854}button:disabled{cursor:default;opacity:.5}
[hidden]{display:none!important}.hint{font-size:14px}#count{color:#d3bcd6;font-size:14px}
</style><main>
<h1>猫咪小屋 · 角色与行为设计</h1>
<p><strong>30 位角色，400 项专属行为，24 项共享行为。</strong> 10 位人类和 20 位动物。小园丁和橘猫各 60 项，其余角色各 10 项。</p>
<p>桌面版按当前条件挑选这些角色短场景，复用像素素材、轨迹和道具。表中「留下的效果」记录完整设计，持久状态目前覆盖种植、灾害和施工等已有系统。原有 1,025 项见 <a href="小世界行为表.html">小世界行为表</a>。</p>
<p class="hint">居民是小园丁和橘猫，同一时段最多接待两位访客。浇水、采收、修补等操作更新有限个地块或道具；便笺、手记和客人记录覆盖旧槽位。小园丁每段工作换装一次，先进屋再出来。橘猫午睡在院里，夜里回屋。夜间时钟按白天流速的两倍走。</p>
<h2>角色名册</h2>
<div class="scroll"><table><thead><tr><th>角色</th><th>类型</th><th>出没与职责</th><th>专属行为</th><th>可共享行为</th></tr></thead><tbody>ROSTER</tbody></table></div>
<h2 id="behavior">行为明细</h2>
<div class="filters"><input id="query" type="search" aria-label="搜索行为" placeholder="搜名字、编号、触发条件或变化">
<select id="role" aria-label="按角色筛选"><option value="">所有角色</option>OPTIONS</select>
<select id="kind" aria-label="行为类型"><option value="">专属和共享</option><option value="exclusive">专属</option><option value="shared">共享</option></select>
<button id="prev">上一页</button><button id="next">下一页</button><span id="count" aria-live="polite"></span></div>
<div class="scroll"><table id="details"><thead><tr><th>编号</th><th>角色</th><th>类型</th><th>行为</th><th>触发条件</th><th>可见变化</th><th>留下的效果</th></tr></thead><tbody id="rows">ROWS</tbody></table></div>
<script>
const query=document.querySelector('#query'),role=document.querySelector('#role'),kind=document.querySelector('#kind');
const prev=document.querySelector('#prev'),next=document.querySelector('#next'),count=document.querySelector('#count');
const rows=[...document.querySelectorAll('#rows tr')].map(row=>({row,roles:row.dataset.role.split(','),kind:row.dataset.kind,text:row.textContent.toLowerCase()}));
let page=0;const size=35;
function filter(){const q=query.value.trim().toLowerCase();let hits=0;
 for(const item of rows){const good=(!role.value||item.roles.includes(role.value))&&(!kind.value||item.kind===kind.value)&&item.text.includes(q);
 const n=good?hits++:-1;item.row.hidden=n<page*size||n>=(page+1)*size;}
 const pages=Math.max(1,Math.ceil(hits/size));prev.disabled=page===0;next.disabled=page+1>=pages;count.textContent=`匹配 ${hits} 项 · 第 ${page+1} / ${pages} 页`;}
for(const input of [query,role,kind])input.addEventListener('input',()=>{page=0;filter()});
prev.addEventListener('click',()=>{page--;filter()});next.addEventListener('click',()=>{page++;filter()});
for(const button of document.querySelectorAll('[data-pick]'))button.addEventListener('click',()=>{role.value=button.dataset.pick;kind.value='';query.value='';page=0;filter();document.querySelector('#behavior').scrollIntoView()});
filter();
</script></main></html>"""
    html = html.replace("ROSTER", "".join(roster)).replace("OPTIONS", "".join(options)).replace("ROWS", "".join(rows))
    Path(sys.argv[1]).write_text(html, encoding="utf-8")
    print(f"{len(profiles)} roles, {len(exclusive)} exclusive, {len(shared)} shared.")


if __name__ == "__main__":
    main()

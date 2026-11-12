"""Export the behavior catalog as a standalone, searchable design table."""
from collections import Counter
from csv import reader
from html import escape
from pathlib import Path
import sys


HERE = Path(__file__).resolve().parent
ROLES = {
    "cat": "猫咪",
    "keeper": "管理员",
    "visitor": "来访者",
    "yard": "庭院与天气",
    "interaction": "手动交互",
}


def read_rows(name):
    with (HERE / name).open(encoding="utf-8-sig", newline="") as source:
        return [row for row in reader(source, delimiter="\t") if row]


def main():
    roles = []
    for number, role, gate, motion, prop, x, y, name, trigger, result in (
        read_rows("animals.tsv") + read_rows("visitors-yard.tsv")
    ):
        roles.append((number, role, name, trigger, result))
    for number, name, result in read_rows("interaction.tsv"):
        roles.append((number, "interaction", name, "手动触发", result))
    counts = Counter(role for _, role, *_ in roles)
    assert len({row[0] for row in roles}) == len(roles) >= 100
    tabs = '<button type="button" data-role="" aria-pressed="true">全部</button>' + "".join(
        f'<button type="button" data-role="{role}" aria-pressed="false">{label} {counts[role]}</button>'
        for role, label in ROLES.items()
    )
    body = "\n".join(
        f'<tr data-role="{role}"><td>{num}</td><td>{ROLES[role]}</td><th scope="row">{escape(name)}</th>'
        f'<td>{escape(trigger)}</td><td>{escape(result)}</td></tr>'
        for num, role, name, trigger, result in roles
    )
    html = f"""<!doctype html>
<html lang="zh-CN">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>猫咪小屋 · 小世界行为表</title>
<style>
:root {{ color-scheme: light; color:#2e3531; background:#f3f2ec; font: 15px/1.55 "Microsoft YaHei", sans-serif; }}
body {{ margin: 0 auto; padding: clamp(14px, 4vw, 48px); max-width: 1250px; }}
h1 {{ font-size: clamp(26px, 4vw, 40px); line-height: 1.2; margin:0 0 8px; }}
p {{ color:#505b51; max-width: 76ch; }}
.controls {{ display:flex; flex-wrap:wrap; align-items:center; gap:8px; margin:20px 0 12px; }}
button, input {{ font: inherit; padding: 7px 11px; border:1px solid #9aa89b; background:#fffefa; color:#2e3531; border-radius:7px; }}
button {{ cursor:pointer; }}
button[aria-pressed=true] {{ background:#365e49; color:white; border-color:#365e49; }}
input {{ flex:1; min-width:190px; }}
.wrap {{ overflow-x:auto; background:#fffefa; border:1px solid #d7dbd2; border-radius:9px; }}
table {{ border-collapse:collapse; width:100%; min-width:720px; }}
caption {{ text-align:left; padding:8px 12px; color:#505b51; }}
thead {{ background:#e0e8dd; }}
th,td {{ text-align:left; vertical-align:top; padding:9px 12px; border-bottom:1px solid #e6e9e1; }}
td:nth-child(-n+2) {{ white-space:nowrap; }}
tbody th {{ min-width:8em; }}
tbody tr:nth-child(even) {{ background:#f7f8f3; }}
</style>
<main>
  <h1>猫咪小屋的小世界</h1>
  <p>昼夜跟随系统时间，天气是庭院模拟。管理员只在固定工时动工，施工结束后保留进度。桌宠关闭期间的建设按排班补算。</p>
  <div class="controls" aria-label="筛选行为">{tabs}<input type="search" aria-label="搜索行为" placeholder="搜行为、天气或道具"></div>
  <div class="wrap">
  <table>
    <caption id="count" aria-live="polite">共 {len(roles)} 项，含 {counts["interaction"]} 项手动交互</caption>
    <thead><tr><th>编号</th><th>角色或场景</th><th>行为</th><th>何时发生</th><th>你会看到什么</th></tr></thead>
    <tbody>{body}</tbody>
  </table>
  </div>
</main>
<script>
const buttons = [...document.querySelectorAll("button[data-role]")];
const rows = [...document.querySelectorAll("tbody tr")];
const search = document.querySelector("input");
let role = "";
function filter() {{
  const q = search.value.trim().toLowerCase();
  let visible = 0;
  for (const row of rows) {{
    row.hidden = (role && row.dataset.role !== role) || !row.textContent.toLowerCase().includes(q);
    if (!row.hidden) visible++;
  }}
  document.querySelector("#count").textContent = "显示 " + visible + " / " + rows.length + " 项";
}}
for (const button of buttons) button.addEventListener("click", () => {{
  role = button.dataset.role;
  for (const other of buttons) other.setAttribute("aria-pressed", String(other === button));
  filter();
}});
search.addEventListener("input", filter);
</script>
</html>
"""
    Path(sys.argv[1]).write_text(html, encoding="utf-8")
    print(f"{len(roles)} rows", dict(counts))


if __name__ == "__main__":
    main()

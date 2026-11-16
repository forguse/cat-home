"""Reconcile every original ID with reviewed current producers and exact bindings.

The earlier cast audit supplies original authorship only, never current readiness.
Capability reviews describe concrete missing chains, not technical impossibility.
"""
import argparse
from collections import Counter
import csv
import io
import json
from pathlib import Path
import tomllib

import audit_cast_rules as original
import compile_legacy_bindings as compiler

ROOT = Path(__file__).resolve().parents[1]
REVIEWS = ROOT / 'docs/behavior-review-1.1.7'
TABLE = ROOT / 'docs/current-behavior-audit.tsv'
SUMMARY = ROOT / 'docs/current-behavior-counts.json'


def read(name):
    return json.loads((REVIEWS / name).read_text(encoding='utf-8'))


def capabilities():
    result = {}
    household = read('household-capabilities.json')
    for code, row in household['supported'].items():
        result[code] = (True, row['producer'], row['full_effect_and_limits'],
                        json.dumps({k: row.get(k) for k in ('parcel', 'at_target', 'periods', 'weather_and_seasons', 'partner')}, ensure_ascii=False))
    for code, row in household['blocked'].items():
        result[code] = (False, '', row['reason'], '')
    for row in read('resident-capabilities.json')['rows']:
        code = row['id']
        assert code not in result
        c = row.get('capability', {})
        result[code] = (row['status'] in ('registered', 'implemented'), c.get('producer', ''),
                       row['decision'] + '。' + c.get('world_effect', ''),
                       json.dumps(c, ensure_ascii=False))
    wildlife = read('wildlife-capabilities.json')
    for row in wildlife['rows']:
        code = row['base']
        assert code not in result
        ok = row['status'] != 'missing'
        result[code] = (ok, row.get('producer', ''), row.get('missing', '') if not ok else
                       json.dumps(row.get('implementation', row.get('capability', row)), ensure_ascii=False),
                       json.dumps(row.get('context', {}), ensure_ascii=False))
    for row in wildlife['shared']:
        # Shared S05/S15 belong to the 24 shared authorships, not extra rows.
        result[row['base']] = (True, row['producer'], json.dumps(row['modes'], ensure_ascii=False),
                               json.dumps(row['context'], ensure_ascii=False))
    assert len(result) == 424, len(result)
    return result


def build():
    caps = capabilities()
    authored = original.build_table()
    with compiler.REGISTER.open(encoding='utf-8', newline='') as source:
        bound = {row['old_id']: row for row in csv.DictReader(source, delimiter='\t')}
    registered_bases = {row['base_id'] for row in bound.values()}
    scenes = {r[0]: r for r in compiler.lines(ROOT / 'design/cast-scenes.tsv')}
    assert registered_bases == {code for code, row in caps.items() if row[0]}, 'Review and actual producer register disagree'
    base_authored = {r['旧ID']: r for r in authored if r['旧ID'] == r['基础ID']}
    rows = []
    for old in authored:
        code, base = old['旧ID'], old['基础ID']
        supported, producer, detail, context = caps[base]
        is_base = code == base
        if code in bound:
            status = '已接入基础行为' if is_base else '已接入精确条件变体'
            reason = bound[code]['implementation']
        elif not supported:
            status = '基础流程待开发' if is_base else '依赖未完成的基础流程'
            reason = detail
        elif ((old['旧天气'] == 'rain' and old['旧季节'] == 'winter')
              or (old['旧天气'] == 'snow' and old['旧季节'] != 'winter')):
            status = '与现版气候矛盾'
            reason = '现版降水在冬季为雪，在其余季节为雨。'
        elif old['旧伴侣'] != base_authored[base]['旧伴侣']:
            status = '待实现指定双方互动'
            reason = '当前基础流程没有证明该指定角色与主体实际同场参与；现有主体独立完成不能代替这段双边互动。'
        else:
            mode = scenes[code][6]
            if mode in ('2', '3'):
                status = '待实现增强效果或持久痕迹'
                reason = '当前已登记行为未证明此增强模式；须同时满足原变化结果及精确时空条件，不能只延长动画或改名字。'
            else:
                status = '待匹配附加效果与时空条件'
                reason = '基础流程已执行，但此原场景的附加效果、实际站位、时段、天气或季节未全部证实；不得按组合数直接开启。'
        rows.append({
            '旧ID': code, '基础ID': base, '类型': old['类型'], '名称': old['旧意图'],
            '原触发': old['原作者条件'], '原动作': old['原作者动作'], '原结果': old['原作者结果'],
            '指定同伴': old['旧伴侣'], '时段': old['旧时段'], '天气': old['旧天气'],
            '季节': old['旧季节'], '位置': old['旧位置'], '变化方式': old['旧变化方式'],
            '变体要求结果': old['旧变化结果'], '当前结论': status, '具体依据或缺口': reason,
            '当前基础实际机制': producer or '尚无完整执行入口', '当前基础实际效果及边界': detail,
            '当前基础可用条件': context or '基础流程待完成，尚无完整可用条件',
        })
    assert len(rows) == len({r['旧ID'] for r in rows}) == 5000
    counts = Counter(r['当前结论'] for r in rows)
    base_count = counts['已接入基础行为']
    variants = counts['已接入精确条件变体']
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
    summary = {'version': version, 'original_ids': 5000, 'original_base': 424,
               'original_variants': 4576, 'registered_base': base_count,
               'registered_variants': variants, 'registered_total': base_count + variants,
               'remaining_total': 5000 - base_count - variants,
               'remaining_variants': 4576 - variants,
               'previous_1_1_6': {'base': 46, 'variants': 41, 'total': 87, 'remaining': 4913},
               'previous_1_1_7': {'base': 100, 'variants': 80, 'total': 180, 'remaining': 4820},
               'new_variants_since_1_1_7': variants - 80,
               'newly_connected': base_count + variants - 87,
               'status_counts': dict(sorted(counts.items())),
               'counting_note': '基础行为为有实际执行流程的旧定义；精确变体复用动作但须额外条件/结果，不能说成独立动画。未接条目不是永久不能开发。'}
    return rows, summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    rows, summary = build()
    buffer = io.StringIO()
    writer = csv.DictWriter(buffer, list(rows[0]), delimiter='\t', lineterminator='\n')
    writer.writeheader(); writer.writerows(rows)
    for path, text in ((TABLE, buffer.getvalue()), (SUMMARY, json.dumps(summary, ensure_ascii=False, indent=2) + '\n')):
        if args.check:
            assert path.read_text(encoding='utf-8') == text, f'{path.name} is stale'
        else:
            path.write_text(text, encoding='utf-8')
    print(json.dumps(summary, ensure_ascii=False))


if __name__ == '__main__':
    main()

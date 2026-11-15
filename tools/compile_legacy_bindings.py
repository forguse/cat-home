"""Compile explicitly supported old IDs; never execute the legacy effect engine."""
import argparse
import csv
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FAMILIES = ROOT / 'assets/maps/village-legacy-families.tsv'
GENERATED = ROOT / 'src/legacy_bindings.rs'
REGISTER = ROOT / 'assets/maps/village-legacy-bindings.tsv'
EXACT = ROOT / 'assets/maps/village-legacy-exact-scenes.tsv'
IDENTITY_ORDER_SHA256 = '4f6828a20bce81b04b77daca418a9ad9da7385fa276b11bdaac08241a01c164f'
EXCLUDED_SCENES = {
    # Dusk ends at 20:00. The 18:00 rain spell cannot follow another wet
    # spell under the deterministic weather rule, and reaches 90-minute
    # impressions only at 20:00. No actual dusk completion can match it.
    'S08~22033',
}


def lines(path):
    return [row.split('\t') for row in path.read_text(encoding='utf-8').splitlines()
            if row and not row.startswith(('#', '@'))]


def role(code):
    assert len(code) == 3 and code[0] in 'HA'
    n = int(code[1:])
    assert 1 <= n <= (10 if code[0] == 'H' else 20)
    return n - 1 + (10 if code[0] == 'A' else 0)


def nums(value):
    return set(map(int, value.split(','))) if value else set()


def compile_tables(family_rows=None):
    bases = {r[0]: r for r in lines(ROOT / 'design/cast-rules.tsv')}
    scenes = lines(ROOT / 'design/cast-scenes.tsv')
    identities = sorted([*bases, *(r[0] for r in scenes)])
    assert len(identities) == len(set(identities)) == 5000
    digest = hashlib.sha256(('\n'.join(identities) + '\n').encode()).hexdigest()
    assert digest == IDENTITY_ORDER_SHA256, 'Original IDs changed: explicit saved-bit migration required'
    ordinal = {name: n for n, name in enumerate(identities)}
    if family_rows is None:
        with FAMILIES.open(encoding='utf-8', newline='') as f:
            family_rows = list(csv.DictReader(f, delimiter='\t'))
    families = sorted(family_rows, key=lambda r: r['base'])
    assert len({r['base'] for r in families}) == len(families)
    exact = {}
    if EXACT.exists():
        with EXACT.open(encoding='utf-8', newline='') as source:
            for item in csv.DictReader(source, delimiter='\t'):
                code = item['old_id']
                assert code not in exact, f'duplicate exact scene {code}'
                assert item['evidence'].strip(), f'missing evidence {code}'
                exact[code] = item['evidence']
    scene_index = {r[0]: r for r in scenes}
    for code in exact:
        assert code in scene_index, f'unknown exact scene {code}'
        original = bases[scene_index[code][1]]
        assert scene_index[code][7] == original[5], f'unproven exact partner {code}'
        assert not (scene_index[code][3] == '2' and scene_index[code][4] == '2'), f'impossible winter rain {code}'
        assert code not in EXCLUDED_SCENES, f'unreachable exact scene {code}'
    groups, rules, report = [], [], []
    labels = {}
    # Keep source labels separate from the exact adapted implementation note.
    for r in scenes:
        labels.setdefault(r[1], r[-1].split(' · ')[0])
    for family in families:
        base = family['base']
        assert base in bases
        original = bases[base]
        for field, allowed in (('variant_modes', {1,2,3}), ('places', set(range(6))),
                               ('weather', set(range(4))), ('periods', set(range(4))),
                               ('seasons', set(range(3)))):
            assert nums(family[field]) <= allowed, f'{base}: invalid {field}'
        roles = [role(code) for code in family['roles'].split(',')]
        assert len(roles) == len(set(roles)), f'{base}: duplicate actor roles'
        mask = sum(1 << n for n in roles)
        required = 255 if original[5] == '-' else role(original[5])
        start = len(rules)
        label = labels.get(base, base)
        # The base is always available for genuine completions outside an old
        # variant's narrow season/time; it does not claim that variant happened.
        rules.append((base, ordinal[base], 255, 255, 255, 255, 0, required, label))
        report.append((base, base, 'base', family['roles'], '', '', '', '', '',
                       original[5], family['binding']))
        for r in scenes:
            if r[1] != base:
                continue
            code, _, period, weather, season, place, mode, partner, label = r
            if code in EXCLUDED_SCENES:
                continue
            p, w, s, l, m = map(int, (period, weather, season, place, mode))
            if code not in exact and not (m in nums(family['variant_modes']) and l in nums(family['places'])
                    and w in nums(family['weather']) and p in nums(family['periods'])
                    and s in nums(family['seasons'])):
                continue
            # The old cold-season Rain contexts are impossible in this world.
            if w == 2 and s == 2:
                continue
            # No implicit nearby person can create a paired completed action.
            if partner != original[5]:
                continue
            rules.append((code, ordinal[code], p, w, s, l, m,
                          255 if partner == '-' else role(partner), label))
            report.append((code, base, 'variant', family['roles'], period, weather,
                           season, place, mode, partner, exact.get(code, family['binding'])))
        display = family.get('display_label', '').strip() or labels.get(base, base)
        assert display and '\n' not in display and '\r' not in display
        if not base.startswith('S'):
            assert set(roles) == {role(base.split('-')[0])}, f'{base}: unrelated actor role'
        groups.append((base, start, len(rules) - start, mask, display))
    header = '// Generated by tools/compile_legacy_bindings.py; stable ordinals of the original 5000 IDs.\n'
    header += 'const GROUPS: &[Group] = &[\n'
    for b, start, length, mask, display in groups:
        header += f'    Group {{ base: {json.dumps(b)}, start: {start}, len: {length}, roles: {mask}, display: {json.dumps(display,ensure_ascii=False)} }},\n'
    header += '];\nconst RULES: &[Rule] = &[\n'
    for code, n, p, w, s, l, m, partner, label in rules:
        header += f'    Rule {{ id: {json.dumps(code)}, ordinal: {n}, period: {p}, weather: {w}, season: {s}, place: {l}, mode: {m}, partner: {partner}, label: {json.dumps(label, ensure_ascii=False)} }},\n'
    header += '];\n'
    tsv = 'old_id\tbase_id\tkind\troles\tperiod\tweather\tseason\tplace\tmode\tpartner\timplementation\n'
    tsv += ''.join('\t'.join(r) + '\n' for r in report)
    return header, tsv, len(groups), len(rules) - len(groups)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    rust, register, base, variant = compile_tables()
    if args.check:
        assert 'integration review' not in register and 'DEV_PENDING:' not in register, 'Unfinished behavior contracts cannot be released'
    for path, content in ((GENERATED, rust), (REGISTER, register)):
        if args.check:
            assert path.read_text(encoding='utf-8') == content, f'{path.name} is stale'
        else:
            path.write_text(content, encoding='utf-8')
    print(f'{base} base bindings + {variant} exact-context variants = {base + variant} registered IDs; actual observations are counted separately')


if __name__ == '__main__':
    main()

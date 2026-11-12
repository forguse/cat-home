"""Write the runnable cast scenarios and expose their authored context labels."""
from collections import defaultdict
from itertools import product
from pathlib import Path
from typing import NamedTuple


HERE = Path(__file__).resolve().parent
PERIODS = ("dawn", "day", "dusk", "night")
WEATHERS = ("clear", "clouds", "rain", "snow")
SEASONS = ("spring", "autumn", "winter")
PERIOD_NAMES = ("黎明", "白天", "黄昏", "夜里")
WEATHER_NAMES = ("晴朗", "多云", "雨中", "下雪")
SEASON_NAMES = ("春季", "秋季", "冬季")
PLACE_NAMES = ("就近", "前院地块", "扩建地块", "目标左侧", "沿目标正面", "目标右侧")
PLOT_TARGETS = {"flower", "tree", "crop", "build", "damage", "garden", "pond"}
PLOT_GATES = {"harvest", "budding", "mature", "sprout", "growing", "seed", "damage",
              "storm", "flood", "frost", "drought", "frame", "finished"}
SHARED_COMPANY = {
    "S01": "ground", "S03": "social", "S05": "flight",
    "S17": "social", "S19": "social", "S24": "social",
}


class Scene(NamedTuple):
    id: str
    base: str
    period: int
    weather: int
    season: int
    place: int
    mode: int
    partner: str
    label: str

    def context(self):
        return " · ".join((SEASON_NAMES[self.season], WEATHER_NAMES[self.weather],
                           PERIOD_NAMES[self.period], PLACE_NAMES[self.place]))


def rows(name):
    return [line.split("\t") for line in (HERE / name).read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.startswith("#")]


def profiles():
    specs = {}
    effects = {}
    for kind, name, *fields in rows("cast-scene-profiles.tsv"):
        if kind == "R":
            quota, shifts, weather, seasons = fields
            specs[name] = (int(quota), shifts.split(","), weather.split(","), seasons.split(","))
        else:
            assert kind == "E" and len(fields) == 6
            effects[name] = list(zip(fields[::2], fields[1::2]))
    assert len(specs) == 30 and len(effects) == 24
    return specs, effects


def paired_profiles():
    return {(lead, companion): (set(kinds.split(",")), description)
            for lead, companion, kinds, description in rows("cast-pairs.tsv")}


def base_labels():
    labels = {}
    for file in ("character-humans.tsv", "character-animals-a.tsv", "character-animals-b.tsv"):
        for code, _, _, _, name, *_ in rows(file):
            labels[code] = name
    for code, _, name, *_ in rows("character-shared.tsv"):
        labels[code] = name
    for line in (HERE / "character-residents.txt").read_text(encoding="utf-8").splitlines():
        if line.startswith("@"):
            prefix, index = line[1:], 0
        elif line.strip():
            index += 1
            labels[f"{prefix}-{index:02d}"] = line.split("|", 1)[0].strip()
    return labels


def base_rules():
    return {code: (gate, target, motion, prop, partner, kind)
            for code, gate, target, motion, prop, partner, kind in
            (line for line in rows("cast-rules.tsv") if not line[0].startswith("@"))}

def shared_contexts():
    specs, _ = profiles()
    traits = {code[1:]: set(tags.split()) for code, _, tags in
              (line for line in rows("cast-rules.tsv") if line[0].startswith("@"))}
    contexts = {}
    for code, eligibility, *_ in rows("character-shared.tsv"):
        required = set(eligibility.split("|"))
        required_company = SHARED_COMPANY.get(code)
        eligible = [role for role, tags in traits.items()
                    if ("all" in required or tags.intersection(required))
                    and (required_company is None or required_company in tags)]
        contexts[code] = {}
        for period, weather, season in product(range(4), range(4), range(3)):
            matching = [role for role in eligible
                        if (role != "A01" or period != 3) and PERIODS[period] in specs[role][1]
                        and WEATHERS[weather] in specs[role][2]
                        and SEASONS[season] in specs[role][3]]
            if len(matching) >= 2:
                contexts[code][period, weather, season] = matching
    return contexts


def phase_fit(gate, period):
    if gate in PERIODS:
        return gate == period
    if gate in {"nap", "build", "fish"}:
        return period == "day"
    if gate == "sleep":
        return period in {"day", "night"}
    if gate == "eat":
        return period in {"day", "dusk"}
    return True


def weather_fit(gate, weather, season):
    if gate in {"rain", "snow"}:
        return weather == gate
    if gate == "wet":
        return weather == "rain"
    if gate == "fish" and weather == "snow":
        return False
    # A snow episode can occur only in winter in the natural-weather schedule.
    return weather != "snow" or season == "winter"


def candidates(base, rule, labels, kinds, conditions, partner="-"):
    gate, target, _, _, _, kind = rule
    periods, weathers, seasons = conditions
    if target in {"build", "pond"} or gate in {"frame", "finished"}:
        places = (0, 2)
    elif target in PLOT_TARGETS or gate in PLOT_GATES:
        places = (0, 1, 2)
    else:
        places = (3, 4, 5)
    for period, weather, season, place, mode in product(periods, weathers, seasons, places, (1, 2, 3)):
        if not phase_fit(gate, period) or not weather_fit(gate, weather, season):
            continue
        if gate in SEASONS and season != gate:
            continue
        if period == "night" and (
            partner == "A01" or base.startswith("A01-") and gate not in {"sleep", "night", "treat"}
        ):
            continue
        if gate == "fish" and base.startswith(("A01-", "H01-")):
            # Fishing owns the residents' animation while they cast or eat.
            continue
        codes = PERIODS.index(period), WEATHERS.index(weather), SEASONS.index(season), place, mode
        key = "".join(map(str, codes))
        code = f"{base}~{key}" if partner == "-" else f"{base}+{partner}~{key}"
        label = f"{labels[base]} · {kinds[kind][mode - 1][0]}"
        yield Scene(code, base, *codes, partner, label)


def choose_spread(groups, count):
    """Balance across source behaviors, then spread across their allowed conditions."""
    groups = [sorted(group, key=score) for group in groups if group]
    result = []
    while len(result) < count:
        cycle = 0
        for group in groups:
            if group:
                result.append(group.pop())
                cycle += 1
                if len(result) == count:
                    break
        if not cycle:
            raise ValueError(f"Only {len(result)} candidates for {count} scenes")
    return result


def score(scene):
    # Deterministic wide coverage of modes, places and climates. Keep some early
    # picks in the starter area, especially for roles with a small quota.
    salt = sum((i + 1) * ord(char) for i, char in enumerate(scene.base + scene.partner))
    value = ((scene.period + 1) * 149 + (scene.weather + 1) * 101
             + (scene.season + 1) * 173 + (scene.place + 1) * 239 + scene.mode * 367 + salt * 13)
    return value % 997, scene.id


def make_scenes():
    specs, effects = profiles()
    originals = base_rules()
    labels = base_labels()
    authored = defaultdict(list)
    shared = []
    for base, rule in originals.items():
        if base.startswith("S"):
            shared.append((base, rule))
        else:
            authored[base[:3]].append((base, rule))
    solo_scenes = []
    for role, (quota, periods, weathers, seasons) in specs.items():
        groups = [list(candidates(base, rule, labels, effects, (periods, weathers, seasons)))
                  for base, rule in authored[role] if rule[4] == "-"]
        solo_scenes += choose_spread(groups, quota - len(authored[role]))
    valid_shared = shared_contexts()
    shared_scenes = choose_spread(
        [[scene for scene in candidates(base, rule, labels, effects, (PERIODS, WEATHERS, SEASONS))
          if (scene.period, scene.weather, scene.season) in valid_shared[base]]
         for base, rule in shared if rule[4] == "-"], 1_100 - len(shared)
    )
    paired_scenes = []
    pair_specs = paired_profiles()
    assert len(pair_specs) == 45
    for (role, partner), (effect_filter, _) in pair_specs.items():
        common = tuple([item for item in first if item in second]
                       for first, second in zip(specs[role][1:], specs[partner][1:]))
        assert all(common), (role, partner)
        groups = [list(candidates(base, rule, labels, effects, common, partner))
                  for base, rule in authored[role]
                  if rule[4] in {"-", partner} and rule[5] in effect_filter]
        paired_scenes += choose_spread(groups, 20)
    scenes = solo_scenes + shared_scenes + paired_scenes
    assert len(solo_scenes) == 2_600 and len(shared_scenes) == 1_076 and len(paired_scenes) == 900
    assert len(scenes) + len(originals) == 5_000
    assert len({scene.id for scene in scenes}) == len(scenes)
    return scenes


def read_scenes():
    return [Scene(code, base, int(period), int(weather), int(season), int(place), int(mode), partner, label)
            for code, base, period, weather, season, place, mode, partner, label in
            (line for line in rows("cast-scenes.tsv") if not line[0].startswith("@"))]


def bucket_key(scene):
    if scene.base.startswith("S"):
        role = 30
    else:
        role = int(scene.base[1:3]) + (-1 if scene.base.startswith("H") else 9)
    climate = (scene.period * len(WEATHERS) + scene.weather) * len(SEASONS) + scene.season
    return role, climate


def main():
    scenes = make_scenes()
    specs, _ = profiles()
    text = []
    for role, (_, periods, weathers, seasons) in specs.items():
        actor = int(role[1:]) + (-1 if role.startswith("H") else 9)
        mask = sum(1 << ((PERIODS.index(period) * 4 + WEATHERS.index(weather)) * 3 + SEASONS.index(season))
                   for period, weather, season in product(periods, weathers, seasons))
        text.append(f"#{actor}\t{mask}")
    last = None
    for scene in sorted(scenes, key=bucket_key):
        bucket = bucket_key(scene)
        if bucket != last:
            text.append(f"@{bucket[0]}\t{bucket[1]}")
            last = bucket
        text.append("\t".join(map(str, scene)))
    text = "\n".join(text) + "\n"
    (HERE / "cast-scenes.tsv").write_text(text, encoding="utf-8", newline="\n")
    print(f"{len(scenes):,} expanded + 424 original = 5,000 executable rules.")


if __name__ == "__main__":
    main()

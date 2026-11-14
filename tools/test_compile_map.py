"""Regression checks for the Willow Village source compiler."""

from contextlib import redirect_stderr, redirect_stdout
from copy import deepcopy
from fractions import Fraction
from io import StringIO
import json
import unittest
from unittest.mock import Mock, patch

import compile_map
from compile_map import Layout, MapError, emit


def item(data, collection, identifier):
    return next(row for row in data[collection] if row['id'] == identifier)


class CompileMapTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = json.loads(compile_map.SOURCE.read_bytes(), parse_float=Fraction)

    def setUp(self):
        self.data = deepcopy(self.source)

    def test_rejects_out_of_bounds_coordinate(self):
        item(self.data, 'pois', 'outside_gate')['tile'][0] = self.data['width']
        with self.assertRaisesRegex(MapError, r'outside_gate\.x: expected integer'):
            Layout(self.data)

    def test_rejects_duplicate_id(self):
        door = item(self.data, 'pois', 'home_xiaohe.door')
        self.data['pois'].append(deepcopy(door))
        with self.assertRaisesRegex(MapError, r'duplicate id home_xiaohe\.door'):
            Layout(self.data)

    def test_rejects_unknown_site_poi(self):
        site = next(row for row in self.data['core_sites'] if row['site'] == 'home_table')
        site['site'] = 'missing_poi'
        with self.assertRaisesRegex(MapError, 'missing_poi: missing POI'):
            Layout(self.data)

    def test_rejects_unknown_reservation_member(self):
        site = next(row for row in self.data['interaction_sites'] if row['site'] == 'home_trade')
        group = next(row for row in site['reservation_groups'] if row['id'] == 'home_trade.greetingpair')
        group['members'].append('missing_member')
        with self.assertRaisesRegex(MapError, r'home_trade\.greetingpair: invalid member references'):
            Layout(self.data)

    def test_rejects_solid_prop_on_door(self):
        door = item(self.data, 'pois', 'home_xiaohe.door')
        self.data['solid_props'].append({'id': 'blocked_door', 'tile': door['tile'], 'phase': 'day_1'})
        with self.assertRaisesRegex(MapError, r'day_1 .* crosses blocked tiles'):
            Layout(self.data)

    def test_rejects_delayed_approach_to_day_one_door(self):
        item(self.data, 'spurs', 'spur_home_xiaohe.door')['phase'] = 'late'
        with self.assertRaisesRegex(MapError, r'active home_xiaohe\.door cannot be reached'):
            Layout(self.data)

    def test_rejects_road_crossing_water_without_approved_bridge(self):
        self.data['bridge_rules'] = [row for row in self.data['bridge_rules'] if row['road'] != 'south_bridge']
        with self.assertRaisesRegex(MapError, r'late .* crosses blocked tiles'):
            Layout(self.data)

    def test_rejects_fishing_foot_anchor_in_water(self):
        self.data['pond']['fishing'].append(self.data['pond']['center'])
        with self.assertRaisesRegex(MapError, 'home_pond: fishing foot anchor is blocked'):
            Layout(self.data)

    def test_rejects_market_material_slot_on_road(self):
        item(self.data, 'buildings', 'market_depot')['material_slot'] = [114, 61]
        with self.assertRaisesRegex(MapError, 'market_depot: material slot overlaps'):
            Layout(self.data)

    def test_rejects_overlapping_stalls_even_if_their_solid_parts_are_distinct(self):
        stall = item(self.data, 'buildings', 'stall_produce')
        stall['rect'][2] = 8
        stall['render_envelope'][2] = 11
        with self.assertRaisesRegex(MapError, 'stall_seed: overlaps another building footprint'):
            Layout(self.data)

    def test_four_weekly_stalls_share_a_short_street_with_two_facing_fronts(self):
        layout = Layout(self.data)
        booths = [layout.buildings[f'stall_{key}'] for key in ('produce', 'seed', 'meal', 'cloth')]
        doors = [(x, y) for x, y in (row['door'] for row in booths)]
        self.assertEqual({(83, 44), (90, 44), (83, 52), (90, 52)}, set(doors))
        self.assertTrue(all(row['phase'] == 'weekly_market' for row in booths))
        _, street, walking = layout.geometry({'day_1', 'weekly_market'})
        self.assertTrue({(x, y) for x in range(78, 96) for y in (48, 49)} <= street & walking)
        for row in booths:
            site = layout.sites[row['id'] + '.door']
            slot = {s['slot']: s for s in site['slots']}
            merchant_y, customer_y = (slot[name]['tile'][1] for name in ('merchant', 'customer'))
            self.assertLess(abs(customer_y - 48), abs(merchant_y - 48))

    def test_rejects_new_scenery_on_a_reserved_approach(self):
        site = next(row for row in self.data['core_sites'] if row['site'] == 'home_table')
        seat = next(row for row in site['slots'] if row['slot'] == 'seat_a')
        self.data['scenic_objects'][0]['tile'] = seat['tile']
        with self.assertRaisesRegex(MapError, 'scenery overlaps reserved map geometry'):
            Layout(self.data)

    def test_rejects_noncolliding_tree_and_colliding_low_grass(self):
        for kind in ('tree', 'grass'):
            with self.subTest(kind=kind):
                self.data = deepcopy(self.source)
                row = next(row for row in self.data['scenic_objects'] if row['kind'] == kind)
                row['solid'] = not row['solid']
                with self.assertRaisesRegex(MapError, f'wrong collision flag for {kind}'):
                    Layout(self.data)

    def test_equivalent_row_order_produces_identical_outputs_at_a_fixed_source_digest(self):
        digest = '0' * 64
        packed_name = compile_map.PACKED.name
        expected = emit(Layout(self.source), digest, packed_name)

        # Ordered geometry, phase IDs, bed assignments and reservation members
        # retain their order. These tables and their named rows are unordered.
        unordered_tables = (
            'regions', 'water', 'roads', 'spurs', 'buildings', 'pois', 'crops', 'lots',
            'residents', 'trips', 'bridge_rules', 'interaction_sites', 'core_sites', 'solid_props', 'scenic_objects',
        )
        for table in unordered_tables:
            self.data[table].reverse()
        for site in self.data['interaction_sites'] + self.data['core_sites']:
            site['slots'].reverse()
            site['reservation_groups'].reverse()

        self.assertEqual(emit(Layout(self.data), digest, packed_name), expected)

    def test_check_rejects_stale_generated_file_without_writing(self):
        generated_rust = Mock(name='generated_rust')
        generated_rust.name = compile_map.RUST.name
        generated_rust.is_file.return_value = True
        generated_rust.read_bytes.return_value = b'stale generated data'
        errors = StringIO()

        with patch.object(compile_map, 'RUST', generated_rust), redirect_stderr(errors), redirect_stdout(StringIO()):
            result = compile_map.main(['--check'])

        self.assertEqual(result, 1)
        self.assertIn(f'{generated_rust.name} is stale', errors.getvalue())
        generated_rust.read_bytes.assert_called_once_with()
        generated_rust.write_bytes.assert_not_called()


if __name__ == '__main__':
    unittest.main()

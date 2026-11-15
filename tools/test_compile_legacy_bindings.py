"""A registration typo must fail the build rather than grant another role."""
import csv
import re
import unittest
from unittest.mock import patch, Mock
import io
import compile_legacy_bindings as compiler


class BindingBoundaryTests(unittest.TestCase):
    def fixture(self):
        with compiler.FAMILIES.open(encoding='utf-8', newline='') as stream:
            return next(dict(row) for row in csv.DictReader(stream, delimiter='\t') if row['base']=='H01-37')

    def test_duplicate_roles_cannot_carry_into_another_role_bit(self):
        row=self.fixture(); row['roles']='H01,H01'
        with self.assertRaisesRegex(AssertionError, 'duplicate actor roles'):
            compiler.compile_tables([row])

    def test_different_profession_cannot_claim_an_exclusive_behavior(self):
        row=self.fixture(); row['roles']='H03'
        with self.assertRaisesRegex(AssertionError, 'unrelated actor role'):
            compiler.compile_tables([row])

    def test_typo_in_condition_does_not_silently_drop_or_enable_scenes(self):
        for field,value in (('places','6'),('weather','4'),('seasons','3'),('periods','4'),('variant_modes','0')):
            row=self.fixture(); row[field]=value
            with self.subTest(field=field),self.assertRaisesRegex(AssertionError, 'invalid '+field):
                compiler.compile_tables([row])

    def test_admitted_identity_order_is_stable_when_families_are_added(self):
        row=self.fixture()
        pattern=r'Rule \{ id: "([^"]+)", ordinal: (\d+)'
        alone=dict(re.findall(pattern,compiler.compile_tables([row])[0]))
        all_rows=dict(re.findall(pattern,compiler.compile_tables()[0]))
        self.assertTrue(alone)
        self.assertEqual(alone,{key:all_rows[key] for key in alone})

    def test_exact_scene_cannot_grant_a_different_partner_or_impossible_weather(self):
        scenes=compiler.lines(compiler.ROOT / 'design/cast-scenes.tsv')
        bases={r[0]:r for r in compiler.lines(compiler.ROOT / 'design/cast-rules.tsv')}
        paired=next(r[0] for r in scenes if r[7]!=bases[r[1]][5])
        winter_rain=next(r[0] for r in scenes if r[3]=='2' and r[4]=='2' and r[7]==bases[r[1]][5])
        for code,reason in ((paired,'partner'),(winter_rain,'winter rain'),('bogus~12345','unknown exact')):
            with self.subTest(code=code):
                path=Mock()
                path.exists.return_value=True
                path.open.return_value=io.StringIO('old_id\tevidence\n'+code+'\texplanation\n')
                with patch.object(compiler,'EXACT',path),self.assertRaisesRegex(AssertionError,reason):
                    compiler.compile_tables([self.fixture()])

    def test_exact_scene_does_not_expand_a_cartesian_condition_grid(self):
        with compiler.FAMILIES.open(encoding='utf-8',newline='') as stream:
            row=next(dict(r) for r in csv.DictReader(stream,delimiter='\t') if r['base']=='H06-01')
        for key in ('variant_modes','places','weather','periods','seasons'):row[key]=''
        path=Mock()
        path.exists.return_value=True
        path.open.return_value=io.StringIO('old_id\tevidence\nH06-01~10041\tsame paid spring meal\n')
        with patch.object(compiler,'EXACT',path):
            result=compiler.compile_tables([row])[1]
        self.assertIn('H06-01~10041',result)
        self.assertNotIn('H06-01~11041',result)


if __name__=='__main__':
    unittest.main()

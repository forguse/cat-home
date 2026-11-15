"""A registration typo must fail the build rather than grant another role."""
import csv
import re
import unittest
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


if __name__=='__main__':
    unittest.main()

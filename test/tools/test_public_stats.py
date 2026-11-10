import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('export_public_stats', Path(__file__).parents[2]/'tools/export_public_stats.py')
export = importlib.util.module_from_spec(spec)
spec.loader.exec_module(export)

class PublicSnapshotTests(unittest.TestCase):
    def setUp(self):
        self.now = 1794326400000
        self.summary = {'at': self.now, 'miners': 0, 'hashrate': 0, 'recent': [], 'history': [], 'boards': []}

    def test_preserves_real_zero_reading_and_excludes_unrecognised_fields(self):
        self.summary.update(private_token='not-public', internal={'addresses': ['private']})
        value = export.public_summary(self.summary, self.now)
        self.assertEqual(value['hashrate'], 0)
        self.assertEqual(value['at'], self.now)
        self.assertNotIn('private_token', value)
        self.assertNotIn('internal', value)

    def test_rejects_invalid_or_stale_input(self):
        for change in ({'at': self.now-300001}, {'at': self.now+60001}, {'miners': True},
                       {'miners': -1}, {'hashrate': float('nan')}, {'recent': [None]}, {'models': {}}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                export.public_summary({**self.summary, **change}, self.now)

    def test_bad_output_cannot_replace_a_previous_reading(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)/'summary.json'
            export.write_summary(target, self.summary)
            original = target.read_bytes()
            with self.assertRaises(ValueError):
                export.write_summary(target, {**self.summary, 'hashrate': float('nan')})
            self.assertEqual(target.read_bytes(), original)
            self.assertEqual(json.loads(original), self.summary)

    def test_oversized_output_cannot_replace_a_previous_reading(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)/'summary.json'
            export.write_summary(target, self.summary)
            with self.assertRaises(ValueError):
                export.write_summary(target, {**self.summary, 'boards': [{'name': 'x'*export.MAX_BYTES}]})
            self.assertEqual(json.loads(target.read_text()), self.summary)

if __name__ == '__main__':
    unittest.main()

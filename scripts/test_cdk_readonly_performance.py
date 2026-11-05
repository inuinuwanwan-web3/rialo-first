"""Offline safety regressions for the benchmark driver; no real workload or RPC."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from phase7z_regression import block_network

RUNNER = Path(__file__).with_name('cdk_readonly_performance.py')


class RunnerSafety(unittest.TestCase):
    def run_driver(self, output, *options):
        return subprocess.run(
            [sys.executable, str(RUNNER), '--binary', '/usr/bin/false',
             '--output', str(output), *options], capture_output=True,
            preexec_fn=block_network, timeout=20)

    def test_first_unexpected_exit_stops_and_records(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)/'result.json'
            p = self.run_driver(output, '--release-core', '--repeats', '20')
            self.assertNotEqual(p.returncode, 0)
            result = json.loads(output.read_text())
            self.assertTrue(result['stopped'])
            self.assertEqual(result['stop_reason'], 'unexpected_result')
            self.assertEqual(len(result['trials']), 1)
            self.assertEqual(result['completed_cases'], [])

    def test_existing_evidence_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)/'result.json'
            output.write_text('preserve')
            self.assertEqual(self.run_driver(output).returncode, 2)
            self.assertEqual(output.read_text(), 'preserve')

    def test_release_requires_twenty_trials(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)/'result.json'
            p = self.run_driver(output, '--release-core', '--repeats', '3')
            self.assertEqual(p.returncode, 2)
            self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()

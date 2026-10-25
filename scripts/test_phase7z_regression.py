"""Runner control-flow tests; no changes to the 189 Rust test definitions."""
import argparse
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('runner', Path(__file__).with_name('phase7z_regression.py'))
r = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(r)


class RunnerTests(unittest.TestCase):
    def test_allowlist_matches_audit_and_rejects_unknown(self):
        manifest = json.loads(r.MANIFEST.read_text())
        r.validate_manifest(manifest)
        self.assertEqual(len(r.select_tests(manifest, [])), 71)
        dangerous = next(x for x in manifest['inventory'] if x['classification'] == 'D')
        for requested in [['lib:unreviewed'], [dangerous['target'] + ':' + dangerous['name']],
                          ['lib:' + manifest['allowed'][0]['name']] * 2]:
            with self.assertRaises(RuntimeError):
                r.select_tests(manifest, requested)

    def test_changed_source_fails_closed(self):
        manifest = json.loads(r.MANIFEST.read_text())
        manifest['source_sha256']['Cargo.toml'] = 'changed'
        with self.assertRaises(RuntimeError):
            r.validate_manifest(manifest)

    def test_network_filter_inherits_to_exec(self):
        code = r'''
import importlib.util, subprocess, sys
spec=importlib.util.spec_from_file_location('r',sys.argv[1]); r=importlib.util.module_from_spec(spec); spec.loader.exec_module(r)
assert r.block_network()['verified']
import ctypes, errno, socket
libc=ctypes.CDLL(None,use_errno=True)
assert libc.connect(-1,None,0)==-1 and ctypes.get_errno()==errno.EPERM
try: socket.socketpair(socket.AF_INET)
except OSError as error: assert error.errno==errno.EPERM
else: raise AssertionError('non-Unix socketpair permitted')
a,b=socket.socketpair();a.close();b.close()
subprocess.run([sys.executable,'-c',"import socket,errno;\nfor d in (socket.AF_INET,socket.AF_INET6,socket.AF_UNIX):\n try: socket.socket(d)\n except OSError as e: assert e.errno==errno.EPERM\n else: raise AssertionError('network available')"],check=True,close_fds=True)
'''
        p = subprocess.run([sys.executable, '-c', code, str(Path(r.__file__))], capture_output=True)
        self.assertEqual(p.returncode, 0, p.stderr.decode())

    def test_timeout_kills_owned_process_group(self):
        with tempfile.TemporaryDirectory(dir='/tmp') as temp:
            marker = Path(temp) / 'survived'
            child = "import time,pathlib;time.sleep(.4);pathlib.Path(" + repr(str(marker)) + ").touch()"
            parent = "import subprocess,sys,time;subprocess.Popen([sys.executable,'-c'," + repr(child) + "]);time.sleep(5)"
            with self.assertRaises(subprocess.TimeoutExpired):
                r.command([sys.executable, '-c', parent], os.environ.copy(), .15)
            time.sleep(.5)
            self.assertFalse(marker.exists())

    def test_result_parser_rejects_failed_missing_and_extra(self):
        good = 'test one ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;'
        r.parse_test(good, 'one', 0)
        for output, code in [(good, 1), ('test one ... FAILED', 101), ('', 0),
                             (good + '\ntest two ... ok', 0)]:
            with self.assertRaises(RuntimeError):
                r.parse_test(output, 'one', code)

    def test_json_never_overwrites(self):
        with tempfile.TemporaryDirectory(dir='/tmp') as temp:
            path = Path(temp) / 'result.json'
            r.save_result(path, {'passed': 1})
            with self.assertRaises(FileExistsError):
                r.save_result(path, {})
            self.assertEqual(json.loads(path.read_text()), {'passed': 1})

    def scenario(self, mode):
        manifest = json.loads(r.MANIFEST.read_text())
        test = manifest['allowed'][0]
        inventory = '\n'.join(x['name'] + ': test' for x in manifest['inventory'] if x['target'] == 'lib')
        # Controlled failure before any later test; other target inventories supplied separately.
        outputs = [(0, json.dumps({'reason': 'compiler-artifact', 'profile': {'test': True},
                                  'target': {'kind': ['lib']}, 'executable': '/mock'})), (0, inventory)]
        reduced = dict(manifest, inventory=[x for x in manifest['inventory'] if x['target'] == 'lib'])
        if mode == 'failed':
            outputs.append((101, 'test ' + test['name'] + ' ... FAILED'))
        else:
            outputs.append(subprocess.TimeoutExpired('mock', 1))
        def command(*args):
            outcome = outputs.pop(0)
            if isinstance(outcome, Exception):
                raise outcome
            return outcome
        with tempfile.TemporaryDirectory(dir='/tmp') as temp:
            path = Path(temp) / 'result.json'
            args = argparse.Namespace(output=str(path), test=[], test_timeout=1, build_timeout=1)
            with patch.object(r, 'block_network', return_value={'verified': True}), \
                 patch.object(r, 'validate_manifest'), patch.object(r, 'command', side_effect=command), \
                 patch.object(r, 'select_tests', return_value=[test, manifest['allowed'][1]]), \
                 patch.object(r.MANIFEST.__class__, 'read_text', autospec=True, return_value=json.dumps(reduced)), \
                 contextlib.redirect_stdout(io.StringIO()):
                code = r.run(args)
            data = json.loads(path.read_text())
            self.assertEqual(code, 1)
            self.assertEqual(data['executed'], 1)
            self.assertEqual(data['failed'], 1)
            self.assertEqual(len(data['unexecuted']), len(reduced['inventory']) - 1)
            self.assertEqual(data['tests'][0]['status'], mode)
            self.assertEqual(outputs, [])

    def test_test_failure_stops_and_returns_nonzero_json(self):
        self.scenario('failed')

    def test_test_timeout_stops_and_records_json(self):
        self.scenario('timeout')

    def test_network_unavailable_runs_no_tests(self):
        with tempfile.TemporaryDirectory(dir='/tmp') as temp:
            path = Path(temp) / 'result.json'
            args = argparse.Namespace(output=str(path), test=[], test_timeout=1, build_timeout=1)
            with patch.object(r, 'block_network', side_effect=RuntimeError('filter unavailable')), \
                 patch.object(r, 'command') as command, contextlib.redirect_stdout(io.StringIO()):
                code = r.run(args)
            data = json.loads(path.read_text())
            self.assertEqual(code, 1)
            self.assertFalse(data['network']['verified'])
            self.assertEqual(data['executed'], 0)
            self.assertEqual(len(data['unexecuted']), 189)
            command.assert_not_called()


if __name__ == '__main__':
    unittest.main(verbosity=2)

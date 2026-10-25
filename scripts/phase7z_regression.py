#!/usr/bin/env python3
"""Fail-closed offline runner for the Phase 7Z audit. Python standard library only."""
import argparse
import ctypes
import datetime
import errno
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / 'scripts/phase7z_allowlist.json'
DENIED = ('socket', 'connect', 'bind', 'listen', 'accept', 'accept4',
          'io_uring_setup', 'io_uring_enter', 'io_uring_register')


def block_network():
    """Kernel filter inherited across fork/exec; fail on missing syscall/library."""
    lib = ctypes.CDLL('libseccomp.so.2', use_errno=True)
    lib.seccomp_init.argtypes = [ctypes.c_uint32]
    lib.seccomp_init.restype = ctypes.c_void_p
    lib.seccomp_syscall_resolve_name.argtypes = [ctypes.c_char_p]
    lib.seccomp_syscall_resolve_name.restype = ctypes.c_int
    lib.seccomp_rule_add.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_int, ctypes.c_uint]
    lib.seccomp_load.argtypes = [ctypes.c_void_p]
    lib.seccomp_release.argtypes = [ctypes.c_void_p]
    ctx = lib.seccomp_init(0x7fff0000)
    if not ctx:
        raise RuntimeError('seccomp initialization failed')
    try:
        for name in DENIED:
            nr = lib.seccomp_syscall_resolve_name(name.encode())
            if nr < 0 or lib.seccomp_rule_add(ctx, 0x50000 | errno.EPERM, nr, 0):
                raise RuntimeError('seccomp rule unavailable: ' + name)
        # Cargo's local jobserver needs Unix socketpair IPC. No external endpoint
        # can be reached through a newly created anonymous Unix pair.
        class Compare(ctypes.Structure):
            _fields_ = [('arg', ctypes.c_uint), ('op', ctypes.c_int),
                        ('a', ctypes.c_uint64), ('b', ctypes.c_uint64)]
        lib.seccomp_rule_add_array.argtypes = [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_int,
                                              ctypes.c_uint, ctypes.POINTER(Compare)]
        nr = lib.seccomp_syscall_resolve_name(b'socketpair')
        cmp = Compare(0, 1, socket.AF_UNIX, 0)  # SCMP_CMP_NE
        if nr < 0 or lib.seccomp_rule_add_array(ctx, 0x50000 | errno.EPERM, nr, 1, ctypes.byref(cmp)):
            raise RuntimeError('socketpair restriction unavailable')
        if lib.seccomp_load(ctx):
            raise RuntimeError('seccomp load failed')
    finally:
        lib.seccomp_release(ctx)
    # No addresses are contacted: creation must fail before any possible connect.
    for domain in (socket.AF_INET, socket.AF_INET6, socket.AF_UNIX):
        try:
            with socket.socket(domain):
                raise RuntimeError('network filter probe unexpectedly succeeded')
        except OSError as error:
            if error.errno != errno.EPERM:
                raise
    return {'mechanism': 'Linux seccomp', 'denied_syscalls': list(DENIED),
            'socket_probes': 'IPv4/IPv6/Unix EPERM',
            'socketpair_policy': 'anonymous AF_UNIX local IPC only', 'verified': True}


def source_files():
    paths = [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock']
    for folder in ('src', 'tests', 'examples'):
        paths += [p for p in (ROOT / folder).rglob('*') if p.is_file()]
    return sorted(paths)


def validate_manifest(manifest):
    paths = source_files()
    if any(p.is_symlink() for p in paths) or {str(p.relative_to(ROOT)) for p in paths} != set(manifest['source_sha256']):
        raise RuntimeError('audited source file set changed: re-audit required')
    current = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
    if current != manifest['source_sha256']:
        raise RuntimeError('audited sources changed: re-audit required')
    if (ROOT / 'build.rs').exists() or (ROOT / '.cargo').exists():
        raise RuntimeError('unreviewed Cargo build script/configuration')
    for name, checksum in manifest['evidence_sha256'].items():
        if hashlib.sha256((ROOT / name).read_bytes()).hexdigest() != checksum:
            raise RuntimeError('audit evidence changed: re-audit required')
    allowed = [(x['target'], x['name']) for x in manifest['allowed']]
    inventory = [(x['target'], x['name']) for x in manifest['inventory']]
    if len(allowed) != 71 or len(set(allowed)) != 71 or len(inventory) != 189 or len(set(inventory)) != 189:
        raise RuntimeError('invalid audit inventory')
    if set(allowed) - set(inventory):
        raise RuntimeError('allowlist not in audited inventory')
    evidence = (ROOT / 'docs/phase-7z-fixture-monitor-validation.md').read_text()
    section = evidence.split('### 今回実行したテスト一覧と結果')[1].split('### 検証範囲')[0]
    recorded = set(re.findall(r'\| (lib|bin|test|example) / `([^`]+)` \|', section))
    if set(allowed) != recorded:
        raise RuntimeError('allowlist does not match audited successful tests')


def select_tests(manifest, requested):
    allowed = manifest['allowed']
    if not requested:
        return allowed
    keys = {x['target'] + ':' + x['name']: x for x in allowed}
    if len(set(requested)) != len(requested) or set(requested) - keys.keys():
        raise RuntimeError('requested test is not allowlisted or is duplicated')
    return [keys[key] for key in requested]


def command(args, env, timeout):
    """Kill and reap the entire owned process group on timeout or interruption."""
    with tempfile.TemporaryFile(mode='w+') as capture:
        p = subprocess.Popen(args, cwd=ROOT, env=env, stdout=capture,
                             stderr=subprocess.STDOUT, close_fds=True, start_new_session=True)
        try:
            p.wait(timeout=timeout)
        except BaseException:
            try:
                os.killpg(p.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            p.wait()
            raise
        capture.seek(0)
        return p.returncode, capture.read()


def parse_test(output, name, code):
    outcomes = re.findall(r'^test (.+) \.\.\. (ok|FAILED|ignored.*)$', output, re.M)
    if outcomes != [(name, 'ok')] or code != 0:
        raise RuntimeError('test failed or unexpected/missing result: ' + name)
    if not re.search(r'test result: ok\. 1 passed; 0 failed; 0 ignored;', output):
        raise RuntimeError('test result count mismatch')


def save_result(path, result):
    # Exclusive creation: never overwrite an existing report or unrelated file.
    with Path(path).open('x') as output:
        json.dump(result, output, ensure_ascii=False, indent=2)
        output.write('\n')


def run(args):
    result = {'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'status': 'blocked', 'network': {'verified': False}, 'tests': [], 'commands': [],
              'error': None, 'total': 189, 'passed': 0, 'failed': 0, 'unexecuted': []}
    manifest = {'inventory': []}
    active = None
    try:
        manifest = json.loads(MANIFEST.read_text())
        result['unexecuted'] = manifest['inventory'].copy()
        result['network'] = block_network()
        validate_manifest(manifest)
        selected = select_tests(manifest, args.test)
        with tempfile.TemporaryDirectory(prefix='phase7z-runner-', dir='/tmp') as temp:
            home = Path(temp) / 'home'
            home.mkdir()
            cargo_home = home / '.cargo'
            cargo_home.mkdir()
            registry = Path.home() / '.cargo/registry'
            (cargo_home / 'registry').symlink_to(registry, target_is_directory=True)
            env = {'PATH': os.environ.get('PATH', '/usr/bin:/bin'), 'HOME': str(home),
                   'CARGO_HOME': str(cargo_home), 'CARGO_NET_OFFLINE': 'true', 'TMPDIR': temp,
                   'RUSTUP_HOME': str(Path.home() / '.rustup'), 'LANG': 'C.UTF-8',
                   'CARGO_TARGET_DIR': str(ROOT / 'target'), 'RUST_BACKTRACE': '0'}
            cargo = shutil.which('cargo')
            if not cargo:
                raise RuntimeError('cargo unavailable')
            build = [cargo, 'test', '--offline', '--locked', '--all-targets', '--no-run', '--message-format=json']
            code, output = command(build, env, args.build_timeout)
            result['commands'].append({'kind': 'compile_only', 'argv': build, 'code': code,
                                       'output': '\n'.join(line for line in output.splitlines() if not line.startswith('{'))})
            if code:
                raise RuntimeError('offline compile failed')
            binaries = {}
            for line in output.splitlines():
                if not line.startswith('{'):
                    continue
                msg = json.loads(line)
                if msg.get('reason') != 'compiler-artifact' or not msg.get('executable'):
                    continue
                if not msg.get('profile', {}).get('test'):
                    continue
                target = msg['target']
                key = 'lib' if 'lib' in target['kind'] else target['kind'][0]
                if key == 'example' and target['name'] != 'phase_7y_readonly':
                    continue
                if key in binaries:
                    raise RuntimeError('duplicate test executable')
                binaries[key] = msg['executable']
            found = set()
            for target, binary in binaries.items():
                code, output = command([binary, '--list'], env, args.test_timeout)
                if code:
                    raise RuntimeError('inventory listing failed')
                found.update((target, n) for n in re.findall(r'^(.+): test$', output, re.M))
            expected = {(x['target'], x['name']) for x in manifest['inventory']}
            if found != expected:
                raise RuntimeError('test inventory changed: re-audit required')
            for test in selected:
                active = test
                started = time.monotonic()
                code, output = command([binaries[test['target']], '--exact', test['name'],
                                        '--test-threads=1'], env, args.test_timeout)
                parse_test(output, test['name'], code)
                result['tests'].append({**test, 'status': 'passed', 'duration_seconds': time.monotonic() - started,
                                        'exit_code': code, 'output': output})
                active = None
            result['status'] = 'passed'
    except BaseException as error:
        result['error'] = str(error) or type(error).__name__
        if active is not None:
            result['tests'].append({**active, 'status': 'timeout' if isinstance(error, subprocess.TimeoutExpired) else 'failed'})
            result['status'] = 'failed'
    result['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    result['passed'] = sum(x['status'] == 'passed' for x in result['tests'])
    result['failed'] = len(result['tests']) - result['passed']
    executed = {(x['target'], x['name']) for x in result['tests']}
    result['unexecuted'] = [{**x, 'reason': 'excluded_by_audit' if x['classification'] == 'D' else 'not_run'}
                            for x in manifest['inventory'] if (x['target'], x['name']) not in executed]
    result['executed'] = len(executed)
    result['unexecuted_count'] = result['total'] - len(executed)
    save_result(args.output, result)
    print(json.dumps({k: result[k] for k in ('status', 'executed', 'passed', 'failed', 'error')}))
    return 0 if result['status'] == 'passed' else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, help='new JSON result file (must not exist)')
    parser.add_argument('--test', action='append', default=[], help='exact allowlisted target:name')
    parser.add_argument('--test-timeout', type=float, default=30)
    parser.add_argument('--build-timeout', type=float, default=300)
    args = parser.parse_args()
    if args.test_timeout <= 0 or args.build_timeout <= 0 or not all(
            __import__('math').isfinite(x) for x in (args.test_timeout, args.build_timeout)):
        parser.error('timeouts must be finite and positive')
    if Path(args.output).exists():
        parser.error('output already exists')
    return run(args)


if __name__ == '__main__':
    sys.exit(main())

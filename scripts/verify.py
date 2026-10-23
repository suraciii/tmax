#!/usr/bin/env python3
"""Run a real Justfile gate and retain output and source provenance outside the repo."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time


def timestamp():
    return datetime.now(timezone.utc).isoformat()


def git(root, *args):
    return subprocess.check_output(['git', *args], cwd=root)


def hash_source(path):
    try:
        content = os.fsencode(os.readlink(path)) if path.is_symlink() else path.read_bytes()
    except FileNotFoundError:
        return None
    return hashlib.sha256(content).hexdigest()


def snapshot(root, evidence, label):
    head = subprocess.run(
        ['git', 'rev-parse', '--verify', '--quiet', 'HEAD'], cwd=root,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    commit = head.stdout.decode().strip() if head.returncode == 0 else None
    # Derive the empty tree for this repository's object format, including SHA-256.
    base = commit or subprocess.check_output(
        ['git', 'hash-object', '-t', 'tree', '--stdin'], input=b'', cwd=root,
    ).decode().strip()
    diff = git(root, 'diff', '--binary', base, '--')
    (evidence / f'{label}.diff').write_bytes(diff)
    files = {}
    for kind, args in (
        ('tracked_sha256', ('--cached',)),
        ('untracked_sha256', ('--others', '--exclude-standard')),
    ):
        files[kind] = {
            os.fsdecode(name): hash_source(root / os.fsdecode(name))
            for name in git(root, 'ls-files', '-z', *args).split(b'\0') if name
        }
    return {
        'commit': commit,
        'unborn_head': commit is None,
        'status': os.fsdecode(git(root, 'status', '--porcelain=v1')),
        'diff_file': f'{label}.diff',
        'diff_sha256': hashlib.sha256(diff).hexdigest(),
        **files,
    }


def source_identity(state):
    return {key: value for key, value in state.items() if key != 'diff_file'}


def write_report(evidence, report):
    (evidence / 'report.json').write_text(json.dumps(report, indent=2) + '\n')


def run(root, evidence, gate='ci'):
    root, evidence = root.resolve(), evidence.resolve()
    if evidence == root or root in evidence.parents:
        raise ValueError('Evidence must be outside the repository')
    if evidence.exists():
        raise ValueError('Evidence path already exists; choose a new directory')
    evidence.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    report = {
        'status': 'running',
        'started_at': timestamp(),
        'repository': str(root),
        'command': ['just', gate],
        'platform': platform.platform(),
        'python_version': platform.python_version(),
        'log_file': 'gate.log',
        'gate_exit_code': None,
        'source_changed': None,
        'exit_code': None,
    }
    process = None
    exit_code = 1
    try:
        with (evidence / 'gate.log').open('wb') as log:
            report['before'] = snapshot(root, evidence, 'before')
            report['tool_versions'] = {
                tool: subprocess.check_output(command, cwd=root, stderr=subprocess.STDOUT)
                .decode().strip()
                for tool, command in {
                    'rustc': ['rustc', '-Vv'],
                    'cargo': ['cargo', '-V'],
                    'just': ['just', '--version'],
                }.items()
            }
            write_report(evidence, report)
            # Just owns dependency ordering, deduplication, arguments, and gate bodies.
            process = subprocess.Popen(
                report['command'], cwd=root,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            )
            while chunk := process.stdout.read1(65536):
                log.write(chunk)
                log.flush()
                sys.stdout.buffer.write(chunk)
                sys.stdout.buffer.flush()
            returncode = process.wait()
            report['gate_exit_code'] = returncode
            exit_code = returncode if returncode >= 0 else 128 - returncode
    except (Exception, KeyboardInterrupt) as error:
        report['error'] = f'{type(error).__name__}: {error}'
        exit_code = 130 if isinstance(error, KeyboardInterrupt) else 1
        print(f"verify: {report['error']}", file=sys.stderr)
    finally:
        if process is not None:
            if process.poll() is None:
                process.terminate()
                process.wait()
            process.stdout.close()
        if 'before' in report:
            try:
                report['after'] = snapshot(root, evidence, 'after')
                report['source_changed'] = (
                    source_identity(report['before']) != source_identity(report['after'])
                )
                if report['source_changed'] and exit_code == 0:
                    exit_code = 1
            except (OSError, subprocess.CalledProcessError) as error:
                report['error'] = f'Final snapshot failed: {error}'
                exit_code = exit_code or 1
                print(f"verify: {report['error']}", file=sys.stderr)
        report['status'] = 'error' if 'error' in report else ('pass' if exit_code == 0 else 'fail')
        report['finished_at'] = timestamp()
        report['seconds'] = round(time.monotonic() - started, 3)
        report['exit_code'] = exit_code
        write_report(evidence, report)
    return exit_code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('evidence_dir', type=Path, help='New directory outside the repository')
    parser.add_argument('--gate', choices=('ci', 'ci-pr'), default='ci')
    args = parser.parse_args()
    try:
        return run(Path(__file__).resolve().parents[1], args.evidence_dir, args.gate)
    except (ValueError, OSError) as error:
        print(f'verify: {error}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())

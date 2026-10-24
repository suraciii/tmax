#!/usr/bin/env python3
"""Behavioral regressions using real Git repositories and Justfile gates."""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parent


class VerificationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='tmax-verify-test-')
        self.addCleanup(temporary.cleanup)
        self.work = Path(temporary.name)
        self.repo = self.work / 'repo'
        (self.repo / 'scripts').mkdir(parents=True)
        for name in ('verify.py', 'check_python_cache.py'):
            shutil.copyfile(SCRIPTS / name, self.repo / 'scripts' / name)
        self.git('init', '--quiet')
        self.git('config', 'user.name', 'Verification Test')
        self.git('config', 'user.email', 'test@example.invalid')
        self.git('config', 'commit.gpgsign', 'false')
        self.git('config', 'core.hooksPath', str(self.work / 'no-hooks'))
        self.justfile('ci:\n    @echo main-gate\nci-pr:\n    @echo pr-gate\n')
        (self.repo / 'source.txt').write_bytes(b'original\n')
        self.evidence = self.work / 'evidence'

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.repo, stderr=subprocess.PIPE)

    def commit(self):
        self.git('add', '-A')
        self.git('commit', '--quiet', '-m', 'fixture')

    def justfile(self, body):
        (self.repo / 'Justfile').write_text(body)

    def invoke(self, evidence=None, *options):
        return subprocess.run(
            [sys.executable, '-B', str(self.repo / 'scripts/verify.py'),
             *options, str(evidence or self.evidence)],
            cwd=self.repo, capture_output=True, text=True, timeout=30,
        )

    def report(self):
        return json.loads((self.evidence / 'report.json').read_text())

    def test_gate_body_runs_and_shared_dependencies_run_once(self):
        self.justfile('''shared:
    @echo shared-output
first: shared
second: shared
ci: first second
    @echo gate-body-output
    exit 7
    @echo unreachable-output
''')
        result = self.invoke()
        self.assertEqual(result.returncode, 7, result.stderr)
        self.assertEqual(result.stdout.count('shared-output'), 1)
        self.assertIn('gate-body-output', result.stdout)
        self.assertNotIn('unreachable-output', result.stdout)
        self.assertEqual(self.report()['status'], 'fail')
        self.assertEqual(self.report()['gate_exit_code'], 7)
        self.assertEqual((self.evidence / 'gate.log').read_text(), result.stdout)

    def test_dependency_failure_stops_the_gate(self):
        self.justfile('''bad:
    @echo failure-output >&2
    exit 3
ci: bad
    @echo unreachable-output
''')
        result = self.invoke()
        self.assertEqual(result.returncode, 3, result.stderr)
        self.assertIn('failure-output', (self.evidence / 'gate.log').read_text())
        self.assertNotIn('unreachable-output', result.stdout)
        self.assertEqual(self.report()['exit_code'], 3)

    def test_pr_gate_does_not_execute_main(self):
        result = self.invoke(None, '--gate', 'ci-pr')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('pr-gate', result.stdout)
        self.assertNotIn('main-gate', result.stdout)

    def test_unborn_repository_records_staged_and_untracked_sources(self):
        self.git('add', 'source.txt')
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        report = self.report()
        self.assertIsNone(report['before']['commit'])
        self.assertEqual(report['before']['tracked_sha256']['source.txt'],
                         hashlib.sha256(b'original\n').hexdigest())
        self.assertEqual(report['before']['untracked_sha256']['Justfile'],
                         hashlib.sha256((self.repo / 'Justfile').read_bytes()).hexdigest())
        self.assertIn('+original', (self.evidence / 'before.diff').read_text())
        self.assertFalse(report['source_changed'])

    def test_dirty_committed_sources_are_measured_without_requiring_clean_tree(self):
        self.commit()
        (self.repo / 'source.txt').write_bytes(b'dirty\n')
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.report()['before']['tracked_sha256']['source.txt'],
                         hashlib.sha256(b'dirty\n').hexdigest())
        self.assertIn('+dirty', (self.evidence / 'before.diff').read_text())
        self.assertFalse(self.report()['source_changed'])

    def test_source_mutation_invalidates_a_successful_gate(self):
        self.justfile("ci:\n    printf 'changed\\n' > source.txt\n")
        self.commit()
        result = self.invoke()
        self.assertEqual(result.returncode, 1, result.stderr)
        report = self.report()
        self.assertEqual(report['gate_exit_code'], 0)
        self.assertEqual(report['status'], 'fail')
        self.assertTrue(report['source_changed'])
        self.assertIn('+changed', (self.evidence / 'after.diff').read_text())

    def test_new_untracked_source_invalidates_a_successful_gate(self):
        self.justfile("ci:\n    printf 'new\\n' > added.txt\n")
        self.commit()
        result = self.invoke()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertTrue(self.report()['source_changed'])
        self.assertEqual(self.report()['after']['untracked_sha256']['added.txt'],
                         hashlib.sha256(b'new\n').hexdigest())

    def test_removed_source_is_recorded_not_treated_as_snapshot_error(self):
        self.justfile('ci:\n    rm source.txt\n')
        self.commit()
        result = self.invoke()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(self.report()['status'], 'fail')
        self.assertIsNone(self.report()['after']['tracked_sha256']['source.txt'])

    def test_symlink_repointing_is_detected_even_when_both_targets_are_missing(self):
        (self.repo / 'link').symlink_to('missing-before')
        self.justfile('ci:\n    python3 -B -c \'from pathlib import Path; p = Path("link"); p.unlink(); p.symlink_to("missing-after")\'\n')
        self.commit()
        result = self.invoke()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(self.report()['gate_exit_code'], 0)
        self.assertTrue(self.report()['source_changed'])
        self.assertEqual(self.report()['after']['tracked_sha256']['link'],
                         hashlib.sha256(b'missing-after').hexdigest())

    def test_ignored_build_outputs_do_not_invalidate_verification(self):
        (self.repo / '.gitignore').write_text('/target/\n')
        self.justfile('ci:\n    mkdir target\n    echo artifact > target/output\n')
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.report()['source_changed'])
        self.assertNotIn('target/output', self.report()['after']['untracked_sha256'])

    def test_failed_final_snapshot_cannot_report_pass(self):
        self.justfile('ci:\n    rm .git/HEAD\n')
        result = self.invoke()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.report()['status'], 'error')
        self.assertEqual(self.report()['gate_exit_code'], 0)
        self.assertIsNone(self.report()['source_changed'])

    def test_evidence_cannot_contaminate_sources_or_overwrite_existing_data(self):
        link = self.work / 'repo-link'
        link.symlink_to(self.repo, target_is_directory=True)
        for path in (self.repo, self.repo / 'nested/evidence', link / 'evidence'):
            with self.subTest(path=path):
                result = self.invoke(path)
                self.assertEqual(result.returncode, 2)
                self.assertIn('outside the repository', result.stderr)
        self.evidence.mkdir()
        saved = self.evidence / 'saved.txt'
        saved.write_text('preserve\n')
        result = self.invoke()
        self.assertEqual(result.returncode, 2)
        self.assertEqual(saved.read_text(), 'preserve\n')
        self.assertFalse((self.evidence / 'report.json').exists())

    def test_cache_guard_rejects_only_tracked_caches(self):
        command = [sys.executable, '-B', str(self.repo / 'scripts/check_python_cache.py')]
        paths = ('scripts/__pycache__/file.py', 'stale.pyc', 'stale.pyo')
        for relative in paths:
            path = self.repo / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('cache\n')
        result = subprocess.run(command, cwd=self.repo, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.git('add', *paths)
        result = subprocess.run(command, cwd=self.repo, capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        for relative in paths:
            self.assertIn(relative, result.stderr)


if __name__ == '__main__':
    unittest.main()

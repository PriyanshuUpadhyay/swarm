#!/usr/bin/env python3
"""Run owner scripts with stub tool boundaries; no install or remote write occurs."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parent
REAL_GIT = shutil.which("git")
STUB = r'''#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys
p = pathlib.Path
name = p(sys.argv[0]).name
a = sys.argv[1:]
root = p(os.environ['CASE_ROOT'])
state_path = root / 'state.json'
s = json.loads(state_path.read_text())
scenario = os.environ.get('SCENARIO', '')
head = s['head']
def save(): state_path.write_text(json.dumps(s))
def fail(): sys.exit(1)
with (root / 'calls').open('a') as log: log.write(name + ' ' + ' '.join(a) + '\n')
if name == 'git':
    if os.environ['MODE'] != 'release': os.execv(os.environ['REAL_GIT'], ['git', *a])
    if a[:1] == ['-C']: a = a[2:]
    if a[0] == 'rev-parse':
        if '--show-toplevel' in a: print(root)
        elif '--short' in a: print(head[:8])
        else: print(head)
    elif a[:2] == ['branch', '--show-current']: print('main')
    elif a[:2] == ['tag', '--points-at']: print(s.get('tag', ''))
    elif a[0] == 'show-ref': sys.exit(1)
    elif a[:2] == ['remote', 'get-url']: print('https://github.com/example/swarm')
    elif a[0] == 'commit': s['commits'] = s.get('commits', 0) + 1
    elif a[:2] == ['tag', '-a']: s['tag'] = a[2]
    elif a[0] == 'push': s['pushes'] = s.get('pushes', 0) + 1
    elif a[0] == 'show': print('2026-01-01T00:00:00Z')
    save()
elif name == 'gh':
    if a[:2] == ['repo', 'view']: print('example/swarm')
    elif a[:2] == ['run', 'list']:
        workflow = a[a.index('--workflow') + 1]
        key = 'poll_' + workflow
        s[key] = s.get(key, 0) + 1
        save()
        if workflow == 'ci.yml':
            status = 'in_progress' if scenario == 'ci-running' and s[key] == 1 else 'completed'
            conclusion = 'failure' if scenario == 'ci-failure' else 'success'
            print(json.dumps([dict(databaseId=41, status=status, conclusion=conclusion, createdAt='2026-01-01T00:00:00Z')]))
        elif workflow == 'background.yml':
            if scenario == 'background-failure': print('https://github.com/example/swarm/actions/runs/7')
        elif scenario == 'poll-error' and s[key] == 1: fail()
        elif scenario == 'timeout' or scenario == 'poll-errors':
            if scenario == 'poll-errors': fail()
            print('[]')
        else:
            conclusion = 'failure' if scenario == workflow else 'success'
            print(json.dumps([dict(databaseId=42, status='completed', conclusion=conclusion, createdAt='2099-01-01T00:00:00Z')]))
    elif a[:2] == ['release', 'view']:
        if scenario == 'release-view-error': fail()
        if scenario == 'read-after-publish-error' and s.get('published') and not s.get('read_failed'):
            s['read_failed'] = True; save(); fail()
        print(json.dumps(dict(isDraft=not s.get('published', False), publishedAt='2026-01-02T00:00:00Z')))
    elif a[:2] == ['release', 'edit']:
        if scenario == 'publish-error': fail()
        s['published'] = True
        s['publications'] = s.get('publications', 0) + 1
        save()
elif name == 'date':
    if a == ['+%s']:
        s['clock'] = s.get('clock', 0) + (1000 if scenario in ('timeout', 'poll-errors') else 1)
        save(); print(s['clock'])
    else: print('2026-01-01T00:00:00Z')
elif name == 'brew':
    if os.environ['MODE'] == 'install':
        if a[0] == '--repository': print(root / 'tap')
        elif a[0] == '--cache': print(root / 'cache')
        elif a[0] == 'list': print('swarm 0.0.19')
        elif a[0] in ('install', 'reinstall'):
            s['formula'] = (root / 'tap/Formula/swarm.rb').read_text(); save()
            if scenario == 'tap-update':
                formula = root / 'tap/Formula/swarm.rb'
                formula.write_text('swarm-0.0.20\n')
                for args in (['add', '.'], ['commit', '-m', 'tap update']):
                    subprocess.run([os.environ['REAL_GIT'], '-c', 'user.name=Test', '-c', 'user.email=test@example.com', '-c', 'commit.gpgsign=false', '-C', str(root / 'tap'), *args], check=True, capture_output=True)
                formula.write_text('stale local formula\n')
            if scenario == 'brew-error': fail()
    else:
        if scenario == 'brew-error' or scenario == 'brew-' + a[0] + '-error': fail()
        if a[0] == 'list': print('swarm ' + s.get('tag', 'v0.0.20')[1:] + ('' if s.get('installed_release') else '.local.abcdef01'))
        if a[0] == 'reinstall': s['installed_release'] = True; save()
elif name == 'cargo':
    if scenario == 'cargo-error': fail()
elif name == 'uname': print('Darwin' if a == ['-s'] else 'arm64')
elif name == 'zig': print('0.16.0')
elif name == 'node': print('v22.0.0')
elif name == 'pnpm': print('11.15.1')
elif name == 'make':
    tree = p(a[a.index('-C') + 1]).parent
    out = tree / 'ui/.build/release/swarm-0.0.19-macos-arm64.tar.gz'
    out.parent.mkdir(parents=True, exist_ok=True); out.write_bytes(b'fake package')
elif name == 'swarm':
    if a == ['__prune-stale-trust', 'schedule-upgrade']:
        sys.exit(1 if scenario == 'cleanup-error' else 0)
    if scenario == 'version-error': fail()
    print('swarm 0.0.20 ' + ('' if scenario == 'empty-hash' else 'deadbee' if scenario == 'wrong-hash' else head[:int(os.environ.get('HASH_LENGTH', '7'))]))
elif name == 'launchctl':
    if scenario == 'hud-error': fail()
elif name in ('sleep', 'swift'): pass
'''


class OwnerScripts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="owner-scripts-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / 'scripts').mkdir()
        for script in SOURCE.glob('*.sh'):
            shutil.copy2(script, self.root / 'scripts' / script.name)
        (self.root / 'Cargo.toml').write_text('[package]\nname = "swarm"\nversion = "0.0.19"\n')
        (self.root / 'Cargo.lock').write_text('[[package]]\nname = "swarm"\nversion = "0.0.19"\n')
        (self.root / 'scripts/install-local.sh').write_text('#!/bin/sh\nexit 1\n')
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        for name in ('git', 'gh', 'date', 'brew', 'cargo', 'sleep', 'launchctl', 'swarm', 'uname', 'zig', 'node', 'pnpm', 'make', 'swift'):
            path = self.bin / name
            path.write_text(STUB); path.chmod(0o755)
        self.state = dict(head='abcdef0123456789abcdef0123456789abcdef01')
        self.env = dict(os.environ, CASE_ROOT=str(self.root), MODE='release', REAL_GIT=REAL_GIT,
                        HOME=str(self.root / 'home'), TMPDIR=str(self.root),
                        PATH=str(self.bin) + os.pathsep + os.environ['PATH'],
                        GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_NOSYSTEM='1')
        plist = self.root / 'home/Library/LaunchAgents/io.github.priyanshuupadhyay.swarm.hud.plist'
        plist.parent.mkdir(parents=True); plist.touch()
        self.save()

    def save(self):
        (self.root / 'state.json').write_text(json.dumps(self.state))

    def run_script(self, script='release.sh', args=('patch',), scenario='', **env):
        result = subprocess.run([str(self.root / 'scripts' / script), *args], env=dict(self.env, SCENARIO=scenario, **env),
                                capture_output=True, text=True, timeout=20)
        self.state = json.loads((self.root / 'state.json').read_text())
        return result

    def calls(self):
        return (self.root / 'calls').read_text()

    def test_release_accepts_seven_and_eight_char_commit(self):
        for length in ('7', '8'):
            with self.subTest(length=length):
                self.state.pop('tag', None); self.save()
                result = self.run_script(HASH_LENGTH=length)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_release_rejects_empty_or_wrong_commit(self):
        for scenario in ('empty-hash', 'wrong-hash'):
            self.state.pop('tag', None); self.save()
            result = self.run_script(scenario=scenario)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('resume ' + self.state['tag'], result.stderr)

    def test_release_refuses_tagged_head(self):
        self.state['tag'] = 'v0.0.19'; self.save()
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('resume v0.0.19', result.stderr)
        self.assertNotIn('commits', self.state)
        self.assertNotIn('pushes', self.state)

    def test_local_failure_does_not_stop_release(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('local install failed', result.stderr)
        self.assertTrue(self.state['published'])

    def test_no_local_skips_the_local_install(self):
        result = self.run_script(args=('patch', '--no-local'))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn('local install failed', result.stderr)
        self.assertTrue(self.state['published'])
        self.assertIn('swarm __prune-stale-trust schedule-upgrade', self.calls())

    def test_cleanup_failure_does_not_stop_release(self):
        result = self.run_script(scenario='cleanup-error')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('could not schedule trust cleanup', result.stderr)

    def test_release_waits_for_a_running_ci(self):
        result = self.run_script(scenario='ci-running')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.state['poll_ci.yml'], 2)
        self.assertTrue(self.state['published'])

    def test_failed_ci_stops_before_the_bump(self):
        result = self.run_script(scenario='ci-failure')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('ci.yml run 41 ended with failure', result.stderr)
        self.assertNotIn('commits', self.state)
        self.assertNotIn('pushes', self.state)

    def test_failed_background_run_warns_and_releases(self):
        result = self.run_script(scenario='background-failure')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('warning: the last background run on main failed: https://github.com/example/swarm/actions/runs/7', result.stderr)
        self.assertTrue(self.state['published'])

    def test_poll_error_retries(self):
        result = self.run_script(scenario='poll-error')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.state['poll_release.yml'], 2)

    def test_post_push_failures_print_resume_command(self):
        for scenario in ('release.yml', 'tap.yml', 'publish-error', 'release-view-error', 'brew-update-error',
                         'brew-upgrade-error', 'brew-reinstall-error', 'brew-list-error', 'hud-error', 'version-error', 'timeout', 'poll-errors'):
            with self.subTest(scenario=scenario):
                self.state = dict(head=self.state['head']); self.save()
                result = self.run_script(scenario=scenario)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(str(self.root / 'scripts/release.sh') + ' resume ' + self.state['tag'], result.stderr)
                if scenario in ('release.yml', 'tap.yml'):
                    self.assertIn('Next command: gh run rerun 42 -R example/swarm', result.stderr)

    def test_resume_twice_has_one_publication_and_no_bump_or_push(self):
        self.state['tag'] = 'v0.0.20'; self.save()
        for _ in range(2):
            result = self.run_script(args=('resume', 'v0.0.20'))
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.state['publications'], 1)
        self.assertNotIn('commits', self.state)
        self.assertNotIn('pushes', self.state)
        self.assertNotIn('cargo ', self.calls())
        self.assertEqual(self.calls().count('brew reinstall'), 1)

    def test_resume_after_publish_read_error_does_not_publish_again(self):
        result = self.run_script(scenario='read-after-publish-error')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('resume v0.0.20', result.stderr)
        result = self.run_script(args=('resume', 'v0.0.20'))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.state['publications'], 1)
        self.assertEqual(self.state['commits'], 1)
        self.assertEqual(self.state['pushes'], 1)

    def install_fixture(self):
        self.env['MODE'] = 'install'
        self.env['HASH_LENGTH'] = '8'
        shutil.copy2(SOURCE / 'install-local.sh', self.root / 'scripts/install-local.sh')
        (self.root / 'cache').mkdir()
        self.git('init', '-b', 'main', str(self.root))
        self.git('-C', str(self.root), 'config', 'core.abbrev', '8')
        self.git('-C', str(self.root), 'add', 'Cargo.toml', 'Cargo.lock', 'scripts')
        self.git('-C', str(self.root), 'commit', '-m', 'fixture')
        self.state['head'] = self.git('-C', str(self.root), 'rev-parse', 'HEAD').strip(); self.save()
        tap = self.root / 'tap'
        self.git('init', '-b', 'main', str(tap))
        (tap / 'Formula').mkdir()
        (tap / 'Formula/swarm.rb').write_text('swarm-0.0.19\n')
        self.git('-C', str(tap), 'add', '.')
        self.git('-C', str(tap), 'commit', '-m', 'tap fixture')

    def git(self, *args):
        return subprocess.run([REAL_GIT, '-c', 'user.name=Test', '-c', 'user.email=test@example.com',
                               '-c', 'commit.gpgsign=false', *args], check=True, capture_output=True, text=True,
                              env=dict(os.environ, GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_NOSYSTEM='1')).stdout

    def test_local_version(self):
        self.install_fixture()
        result = self.run_script('install-local.sh', ('HEAD',))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('0.0.19.local.', self.state['formula'])
        self.assertIn('swarm __prune-stale-trust schedule-upgrade', self.calls())

    def test_cleanup_failure_does_not_stop_install(self):
        self.install_fixture()
        result = self.run_script('install-local.sh', ('HEAD',), scenario='cleanup-error')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('could not schedule trust cleanup', result.stderr)

    def test_local_accepts_short_commit(self):
        self.install_fixture()
        result = self.run_script('install-local.sh', ('HEAD',), HASH_LENGTH='7')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_tap_update_restores_current_head(self):
        self.install_fixture()
        result = self.run_script('install-local.sh', ('HEAD',), scenario='tap-update')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / 'tap/Formula/swarm.rb').read_text(), 'swarm-0.0.20\n')
        self.assertEqual(self.git('-C', str(self.root / 'tap'), 'status', '--porcelain'), '')
        self.assertFalse((self.root / 'cache/swarm-install-local.lock').exists())

    def test_install_failure_restores_and_cleans(self):
        self.install_fixture()
        result = self.run_script('install-local.sh', ('HEAD',), scenario='brew-error')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.root / 'tap/Formula/swarm.rb').read_text(), 'swarm-0.0.19\n')
        self.assertFalse((self.root / 'cache/swarm-install-local.lock').exists())
        self.assertEqual(len(list(self.root.glob('swarm-install.*'))), 0)

    def test_local_rejects_empty_or_wrong_commit(self):
        self.install_fixture()
        for scenario in ('empty-hash', 'wrong-hash'):
            with self.subTest(scenario=scenario):
                result = self.run_script('install-local.sh', ('HEAD',), scenario=scenario)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.git('-C', str(self.root / 'tap'), 'status', '--porcelain'), '')

    def test_prune_keeps_new_branch_with_tmp_files(self):
        self.env['MODE'] = 'prune'
        self.git('init', '-b', 'main', str(self.root))
        self.git('-C', str(self.root), 'add', 'Cargo.toml', 'scripts')
        self.git('-C', str(self.root), 'commit', '-m', 'fixture')
        self.git('-C', str(self.root), 'remote', 'add', 'origin', str(self.root))
        self.git('-C', str(self.root), 'fetch', 'origin')
        tree = self.root / 'fresh'
        self.git('-C', str(self.root), 'worktree', 'add', '-b', 'fresh-task', str(tree))
        (tree / 'tmp/flow').mkdir(parents=True)
        (tree / 'tmp/flow/01-frame.md').write_text('keep this task\n')
        done = self.root / 'done'
        self.git('-C', str(self.root), 'worktree', 'add', '-b', 'done-task', str(done))
        (done / 'done.txt').write_text('own commit\n')
        self.git('-C', str(done), 'add', 'done.txt')
        self.git('-C', str(done), 'commit', '-m', 'task')
        self.git('-C', str(self.root), 'merge', '--no-ff', 'done-task', '-m', 'merge task')
        # The fresh branch still points to the earlier main commit.
        result = self.run_script('prune-worktrees.sh', ())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f'KEEP {tree} — no commits yet', result.stdout)
        self.assertNotIn(f'REMOVE {tree}', result.stdout)
        self.assertIn(f'REMOVE {done}', result.stdout)
        self.assertTrue((tree / 'tmp/flow/01-frame.md').is_file())


if __name__ == '__main__':
    unittest.main()

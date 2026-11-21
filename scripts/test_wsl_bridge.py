import importlib.util
import json
import os
import pathlib
import select
import signal
import subprocess
import sys
import time
import unittest
from unittest.mock import patch

MODULE = pathlib.Path(__file__).with_name("wsl_bridge.py")


class BridgeTests(unittest.TestCase):
    def test_shell_environment_only_imports_runtime_allowlist(self):
        spec = importlib.util.spec_from_file_location("wsl_bridge", MODULE)
        bridge = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(bridge)
        environment = b'PATH=/expected/node/bin\0https_proxy=http://localhost:7890\0CODEX_HOME=/existing/codex\0PRIVATE_ACCESS_TOKEN=private\0'
        with patch.dict(os.environ, {}, clear=True):
            bridge.apply_shell_environment(environment)
            self.assertEqual(os.environ['PATH'], '/expected/node/bin')
            self.assertEqual(os.environ['CODEX_HOME'], '/existing/codex')
            self.assertIn('https_proxy', os.environ)
            self.assertNotIn('PRIVATE_ACCESS_TOKEN', os.environ)

    def start_bridge(self, server):
        wrapper = f"""
import sys
namespace = {{'__name__': 'test_bridge'}}
exec(compile({MODULE.read_text()!r}, 'wsl_bridge.py', 'exec'), namespace)
namespace['locate_codex'] = lambda: [sys.executable, '-u', '-c', {server!r}]
sys.argv = ['wsl_bridge']
sys.exit(namespace['main']())
"""
        return subprocess.Popen([sys.executable, '-u', '-c', wrapper], stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def readline(self, bridge):
        self.assertTrue(select.select([bridge.stdout], [], [], 5)[0], '桥接未返回输出')
        return bridge.stdout.readline()

    def test_forwards_typed_messages_while_parent_pipe_is_open(self):
        server = """
import sys
for line in sys.stdin.buffer:
    sys.stdout.buffer.write(line)
    sys.stdout.buffer.flush()
"""
        with self.start_bridge(server) as bridge:
            try:
                for message in [{'id': 1, 'method': 'initialize'}, {'id': 2, 'method': 'account/rateLimits/read'}]:
                    bridge.stdin.write((json.dumps(message) + '\n').encode())
                    bridge.stdin.flush()
                    self.assertEqual(json.loads(self.readline(bridge)), message)
            finally:
                bridge.stdin.close()
                bridge.stdin = None
                _, errors = bridge.communicate(timeout=5)
            self.assertEqual(bridge.returncode, 0, errors.decode())

    def test_parent_eof_reaps_process_group_including_descendants_ignoring_sigterm(self):
        server = """
import os, subprocess, sys, time
child = subprocess.Popen([sys.executable, '-u', '-c',
    'import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);print("ready",flush=True);time.sleep(30)'],
    stdout=subprocess.PIPE)
child.stdout.readline()
print(os.getpid(), child.pid, flush=True)
time.sleep(30)
"""
        with self.start_bridge(server) as bridge:
            group = None
            try:
                group, child = map(int, self.readline(bridge).split())
                bridge.stdin.close()
                bridge.stdin = None
                _, errors = bridge.communicate(timeout=5)
                self.assertEqual(bridge.returncode, 0, errors.decode())
                status = pathlib.Path(f'/proc/{child}/status')
                deadline = time.monotonic() + 1
                while True:
                    try:
                        if '\nState:\tZ' in status.read_text():
                            break
                    except FileNotFoundError:
                        break
                    self.assertLess(time.monotonic(), deadline, 'app-server 后代仍在运行')
                    time.sleep(0.01)
            finally:
                if group is not None:
                    try:
                        os.killpg(group, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                if bridge.poll() is None:
                    bridge.kill()


if __name__ == "__main__":
    unittest.main()

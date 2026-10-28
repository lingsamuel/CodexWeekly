import importlib.util
import json
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

MODULE = pathlib.Path(__file__).with_name("wsl_bridge.py")


class BridgeTests(unittest.TestCase):
    def bridge(self):
        spec = importlib.util.spec_from_file_location("wsl_bridge", MODULE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_initializes_before_query_and_ignores_notifications(self):
        bridge = self.bridge()
        server = """
import json,sys
init=json.loads(sys.stdin.readline())
assert init['method']=='initialize'
print(json.dumps({'method':'notice','params':{}}),flush=True)
print(json.dumps({'id':init['id'],'result':{}}),flush=True)
assert json.loads(sys.stdin.readline())['method']=='initialized'
query=json.loads(sys.stdin.readline())
assert query['method']=='account/rateLimits/read'
print(json.dumps({'id':query['id'],'result':{'accountId':'test-account','rateLimits':{'primary':{'usedPercent':27,'windowDurationMins':10080}}}}),flush=True)
sys.stdin.read()
"""
        result = bridge.query([sys.executable, "-u", "-c", server], timeout=2)
        self.assertEqual(result["rateLimits"]["primary"]["usedPercent"], 27)
        self.assertNotIn("accountId", result)
        self.assertEqual(len(result["accountKey"]), 64)

    def test_shell_environment_only_imports_runtime_allowlist(self):
        bridge=self.bridge()
        environment=b'PATH=/expected/node/bin\0https_proxy=http://localhost:7890\0CODEX_HOME=/existing/codex\0PRIVATE_ACCESS_TOKEN=private\0'
        with patch.dict(os.environ,{},clear=True):
            bridge.apply_shell_environment(environment)
            self.assertEqual(os.environ['PATH'],'/expected/node/bin')
            self.assertEqual(os.environ['CODEX_HOME'],'/existing/codex')
            self.assertIn('https_proxy',os.environ)
            self.assertNotIn('PRIVATE_ACCESS_TOKEN',os.environ)

    def test_bridge_exits_cleanly_while_parent_pipe_remains_open(self):
        server="""
import json,sys
for line in sys.stdin:
 req=json.loads(line)
 if req['method']=='initialized':continue
 value={} if req['method']=='initialize' else {'accountId':'a','rateLimits':{'primary':{'usedPercent':10,'windowDurationMins':10080}}}
 print(json.dumps({'id':req['id'],'result':value}),flush=True)
"""
        wrapper=f"""
import sys
namespace={{'__name__':'test_bridge'}}
exec(compile({MODULE.read_text()!r},'wsl_bridge.py','exec'),namespace)
namespace['locate_codex']=lambda:[sys.executable,'-u','-c',{server!r}]
sys.argv=['wsl_bridge','--watch-parent']
sys.exit(namespace['main']())
"""
        process=subprocess.Popen([sys.executable,'-u','-c',wrapper],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        try:
            self.assertIn('result',json.loads(process.stdout.readline()))
            self.assertEqual(process.wait(timeout=5),0,process.stderr.read().decode())
        finally:
            if process.poll() is None:process.kill()
            process.stdin.close()
            process.stdin = None
            process.communicate()

    def test_timeout_is_bounded_and_child_is_reaped(self):
        bridge = self.bridge()
        with self.assertRaisesRegex(bridge.BridgeError, "超时"):
            bridge.query([sys.executable, "-u", "-c", "import time;time.sleep(30)"], timeout=0.15)

    def test_rpc_errors_do_not_echo_private_payloads(self):
        bridge = self.bridge()
        server = "import sys,json;sys.stdin.readline();print(json.dumps({'id':1,'error':{'message':'private-token@example.org'}}),flush=True)"
        with self.assertRaises(bridge.BridgeError) as raised:
            bridge.query([sys.executable, "-u", "-c", server], timeout=2)
        self.assertNotIn("private-token", str(raised.exception))

    def test_shutdown_during_response_is_reported(self):
        bridge = self.bridge()
        with self.assertRaisesRegex(bridge.BridgeError, "退出|断开"):
            bridge.query([sys.executable, "-u", "-c", "pass"], timeout=2)

    def test_legacy_identity_falls_back_to_read_only_account_query(self):
        bridge = self.bridge()
        server = """
import sys,json
for line in sys.stdin:
 req=json.loads(line)
 if req['method']=='initialize': value={}
 elif req['method']=='initialized': continue
 elif req['method']=='account/rateLimits/read': value={'rateLimits':{'primary':{'usedPercent':20,'windowDurationMins':10080}}}
 elif req['method']=='account/read': value={'account':{'type':'chatgpt','email':'private@example.org'}}
 print(json.dumps({'id':req['id'],'result':value}),flush=True)
"""
        result=bridge.query([sys.executable,"-u","-c",server],timeout=2)
        self.assertEqual(len(result['accountKey']),64)
        self.assertNotIn('email',json.dumps(result))

    def test_descendant_ignoring_sigterm_does_not_remain_running(self):
        bridge = self.bridge()
        with tempfile.TemporaryDirectory() as directory:
            pidfile=pathlib.Path(directory)/'pid'
            server=f"""
import subprocess,sys,json,pathlib
p=subprocess.Popen([sys.executable,'-u','-c','import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);print("ready",flush=True);time.sleep(30)'],stdout=subprocess.PIPE)
p.stdout.readline()
pathlib.Path({str(pidfile)!r}).write_text(str(p.pid))
for line in sys.stdin:
 req=json.loads(line)
 if req['method']=='initialize': value={{}}
 elif req['method']=='initialized': continue
 else: value={{'accountId':'a','rateLimits':{{'primary':{{'usedPercent':10,'windowDurationMins':10080}}}}}}
 print(json.dumps({{'id':req['id'],'result':value}}),flush=True)
"""
            try:
                bridge.query([sys.executable,'-u','-c',server],timeout=2)
                pid=int(pidfile.read_text())
                deadline=time.monotonic()+1
                while time.monotonic()<deadline:
                    status=pathlib.Path(f'/proc/{pid}/status')
                    if not status.exists() or '\nState:\tZ' in status.read_text():break
                    time.sleep(0.01)
                else:self.fail('app-server descendant remained running after query cleanup')
            finally:
                if pidfile.exists():
                    try:
                        import os,signal
                        os.kill(int(pidfile.read_text()),signal.SIGKILL)
                    except ProcessLookupError:pass

    def test_parent_closing_during_cleanup_does_not_interrupt_process_reaping(self):
        with tempfile.TemporaryDirectory() as directory:
            pidfile = pathlib.Path(directory) / 'pid'
            cleanup_started = pathlib.Path(directory) / 'cleanup-started'
            server = f"""
import json,os,pathlib,signal,sys,time
pathlib.Path({str(pidfile)!r}).write_text(str(os.getpid()))
def terminating(*args):
 pathlib.Path({str(cleanup_started)!r}).write_text('received SIGTERM')
signal.signal(signal.SIGTERM,terminating)
for line in sys.stdin:
 req=json.loads(line)
 if req['method']=='initialized': continue
 result={{}} if req['method']=='initialize' else {{'accountId':'fake','rateLimits':{{'primary':{{'usedPercent':10,'windowDurationMins':10080}}}}}}
 print(json.dumps({{'id':req['id'],'result':result}}),flush=True)
while True: time.sleep(1)
"""
            wrapper = f"""
import sys
namespace={{'__name__':'review_bridge'}}
exec(compile({MODULE.read_text()!r},'wsl_bridge.py','exec'),namespace)
namespace['locate_codex']=lambda:[sys.executable,'-u','-c',{server!r}]
sys.argv=['wsl_bridge','--watch-parent']
sys.exit(namespace['main']())
"""
            bridge = subprocess.Popen([sys.executable, '-u', '-c', wrapper],
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE)
            try:
                deadline = time.monotonic() + 5
                while not cleanup_started.exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue(cleanup_started.exists(), 'fake app-server did not enter cleanup')
                # 首次 SIGTERM 已进入 wait；此时关闭父管道，精确覆盖取消与回收的交界。
                bridge.stdin.close()
                bridge.stdin = None
                output, errors = bridge.communicate(timeout=5)
                pid = int(pidfile.read_text())
                status = pathlib.Path(f'/proc/{pid}/status')
                self.assertFalse(status.exists() and '\nState:\tZ' not in status.read_text(),
                                 'parent EOF interrupted cleanup and left app-server running')
                self.assertEqual(bridge.returncode, 0, errors.decode())
                self.assertIn('result', json.loads(output))
            finally:
                if pidfile.exists():
                    try:
                        os.killpg(int(pidfile.read_text()), signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                if bridge.poll() is None:
                    bridge.kill()
                bridge.communicate()


if __name__ == "__main__":
    unittest.main()

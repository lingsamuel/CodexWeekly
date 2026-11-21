"""恢复 WSL 用户的 Codex 环境并桥接标准输入输出。"""
import json
import os
import pathlib
import pwd
import shutil
import signal
import subprocess
import sys
import threading


class BridgeError(Exception):
    pass


def stop_process_group(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        pass
    # 根 launcher 退出不代表所有后代都退出。
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


def apply_shell_environment(output):
    allowed = {'PATH', 'CODEX_HOME', 'NVM_DIR', 'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY',
               'NO_PROXY', 'http_proxy', 'https_proxy', 'all_proxy', 'no_proxy'}
    for entry in output.split(b'\0'):
        key, separator, value = entry.partition(b'=')
        name = key.decode('utf-8', errors='replace')
        if separator and name in allowed:
            os.environ[name] = value.decode('utf-8', errors='surrogateescape')


def load_shell_environment():
    # Windows wsl.exe --exec 不加载 .zshrc/NVM/代理；仅在 WSL 内恢复运行需要的白名单。
    shell = pwd.getpwuid(os.getuid()).pw_shell
    if not shell or not pathlib.Path(shell).is_file():
        return
    process = subprocess.Popen([shell, '-ic', 'command env -0'], stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, start_new_session=True)
    try:
        output, _ = process.communicate(timeout=5)
        if process.returncode == 0:
            apply_shell_environment(output)
    except subprocess.TimeoutExpired:
        raise BridgeError('WSL shell 初始化超时，请检查 shell 启动配置是否等待交互。') from None
    finally:
        stop_process_group(process)
        process.stdout.close()


def locate_codex():
    found = shutil.which("codex")
    if found:
        return [found, "app-server", "--listen", "stdio://"]
    # wsl.exe --exec 不加载交互 shell；NVM 安装的 Node 通常不在默认 PATH 中。
    roots = [pathlib.Path.home() / ".nvm" / "versions" / "node"]
    if os.environ.get("NVM_DIR"):
        roots.insert(0, pathlib.Path(os.environ["NVM_DIR"]) / "versions" / "node")
    candidates = []
    for root in roots:
        if root.is_dir():
            candidates.extend(root.glob("*/bin/codex"))
    candidates.sort(key=lambda item: item.stat().st_mtime, reverse=True)
    for executable in candidates:
        if executable.is_file() and os.access(executable, os.X_OK):
            os.environ["PATH"] = str(executable.parent) + os.pathsep + os.environ.get("PATH", "")
            return [str(executable), "app-server", "--listen", "stdio://"]
    for executable in [pathlib.Path.home() / ".local/bin/codex", pathlib.Path.home() / ".cargo/bin/codex"]:
        if executable.is_file() and os.access(executable, os.X_OK):
            return [str(executable), "app-server", "--listen", "stdio://"]
    raise BridgeError("此 WSL 发行版中未找到 Codex CLI。请安装 Codex 并沿用已有的 ChatGPT 登录。")


def relay_quota_stdio(command):
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stderr=subprocess.DEVNULL,
                               start_new_session=True)
    parent_closed = threading.Event()

    def forward_input():
        try:
            while True:
                chunk = os.read(sys.stdin.fileno(), 65536)
                if not chunk:
                    break
                process.stdin.write(chunk)
                process.stdin.flush()
        except OSError:
            pass
        finally:
            parent_closed.set()

    try:
        # Codex 直接写入祖先进程的 stdout。输入单独转发，取消时用 EOF 回收整个进程组。
        threading.Thread(target=forward_input, daemon=True).start()
        while not parent_closed.wait(0.05):
            status = process.poll()
            if status is not None:
                return status
        return 0
    finally:
        stop_process_group(process)
        process.stdin.close()


def main():
    try:
        if '--restore-shell-environment' in sys.argv:
            load_shell_environment()
        command = locate_codex()
        if '--daemon-proxy' in sys.argv:
            # 代理保持原始 WebSocket 字节；关闭时既有 daemon 继续运行。
            os.execvpe(command[0], [command[0], 'app-server', 'proxy'], os.environ)
        return relay_quota_stdio(command)
    except BridgeError as error:
        print(json.dumps({"bridgeError": str(error)}, ensure_ascii=False), flush=True)
    except Exception:
        print(json.dumps({"bridgeError": "WSL 额度桥接启动失败，请检查 Python 3 与 Codex CLI。"},
                         ensure_ascii=False), flush=True)
    return 1


if __name__ == "__main__":
    sys.exit(main())

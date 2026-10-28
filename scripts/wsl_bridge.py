"""通过当前 WSL 用户的 Codex 读取额度；只依赖 Python 标准库。"""
import hashlib
import json
import os
import pathlib
import pwd
import selectors
import shutil
import signal
import subprocess
import sys
import threading
import time


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


def query(command, timeout=18, watch_parent=False):
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.DEVNULL, start_new_session=True)
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    deadline = time.monotonic() + timeout
    buffer = bytearray()

    def send(message):
        process.stdin.write((json.dumps(message) + "\n").encode())
        process.stdin.flush()

    def receive(request_id):
        while time.monotonic() < deadline:
            # 先消费缓冲区，再等待 fd，避免通知与响应在一次读取中到达时卡住。
            while b"\n" in buffer:
                line, _, rest = buffer.partition(b"\n")
                buffer[:] = rest
                try:
                    value = json.loads(line)
                except (ValueError, UnicodeError):
                    raise BridgeError("Codex 返回了无法识别的协议数据，请更新 Codex CLI。") from None
                if not isinstance(value, dict) or value.get("id") != request_id:
                    continue
                if "error" in value:
                    error = value["error"]
                    message = str(error.get("message", "")).lower() if isinstance(error, dict) else ""
                    if any(word in message for word in ["auth", "login", "sign in", "unauthorized", "401"]):
                        raise BridgeError("当前 WSL Codex 登录已失效，请先在该 WSL 中恢复 Codex 登录。")
                    raise BridgeError("Codex 无法读取订阅额度，请检查当前 WSL 的 Codex 登录与网络。")
                result = value.get("result")
                if not isinstance(result, dict):
                    raise BridgeError("Codex 返回了不完整的额度响应。")
                return result
            if not selector.select(max(0, deadline - time.monotonic())):
                break
            chunk = os.read(process.stdout.fileno(), 65536)
            if not chunk:
                raise BridgeError("Codex app-server 已退出或连接断开，请检查 WSL 中的 Codex 是否可用。")
            buffer.extend(chunk)
            if len(buffer) > 2 * 1024 * 1024:
                raise BridgeError("Codex 协议响应过大，已停止本次读取。")
        raise BridgeError("读取 Codex 额度超时，请检查 WSL 网络或稍后刷新。")

    try:
        if watch_parent:
            def parent_closed():
                # 不持有 BufferedReader 锁，允许主线程成功退出时终止此守护线程。
                while os.read(sys.stdin.fileno(), 1024):
                    pass
                os.kill(os.getpid(), signal.SIGTERM)
            threading.Thread(target=parent_closed, daemon=True).start()
        send({"id": 1, "method": "initialize", "params": {"clientInfo": {
            "name": "codex_weekly_overlay", "title": "Codex Weekly Overlay", "version": "0.1.0"}}})
        receive(1)
        send({"method": "initialized", "params": {}})
        send({"id": 2, "method": "account/rateLimits/read"})
        result = receive(2)
        identity = result.get("accountId")
        if not identity:
            send({"id": 3, "method": "account/read", "params": {"refreshToken": False}})
            account = receive(3).get("account") or {}
            identity = account.get("id") or account.get("email")
        if not isinstance(identity, str) or not identity:
            raise BridgeError("无法识别当前 Codex 账号，已停止记录以避免混合不同账号的历史。")
        # 只输出额度和不可逆的账号分组键；服务返回的身份、重置券等不进入 Windows 前端。
        return {"accountKey": hashlib.sha256(identity.encode()).hexdigest(),
                "rateLimits": result.get("rateLimits"),
                "rateLimitsByLimitId": result.get("rateLimitsByLimitId")}
    except (BrokenPipeError, OSError):
        raise BridgeError("Codex app-server 连接断开，请确认当前 WSL 中可以运行 Codex。") from None
    finally:
        if watch_parent:
            # 父管道可在回收期间关闭；此时取消信号不能再次抛异常打断 kill/reap。
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
        selector.close()
        # 每次采集结束都关闭独立 app-server 的进程组，空闲时不驻留额外 Codex 实例。
        stop_process_group(process)
        process.stdin.close()
        process.stdout.close()


def main():
    def cancelled(_number, _frame):
        raise BridgeError("额度采集已取消。")
    signal.signal(signal.SIGTERM, cancelled)
    try:
        if '--restore-shell-environment' in sys.argv:
            load_shell_environment()
        print(json.dumps({"result": query(locate_codex(), watch_parent="--watch-parent" in sys.argv)}, ensure_ascii=False), flush=True)
    except BridgeError as error:
        print(json.dumps({"error": str(error)}, ensure_ascii=False), flush=True)
        return 1
    except Exception:
        print(json.dumps({"error": "WSL 额度桥接启动失败，请检查 Python 3 与 Codex CLI。"}, ensure_ascii=False), flush=True)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

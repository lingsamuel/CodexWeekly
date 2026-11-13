"""组装可移植 Windows 包；GNU 目标包含它实际依赖的运行库。"""
import argparse
import hashlib
import pathlib
import re
import shutil
import subprocess
import tomllib
import zipfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", default="x86_64-pc-windows-gnu")
    args = parser.parse_args()
    root = pathlib.Path(__file__).resolve().parents[1]
    version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    release_name = f"CodexWeekly-{version}-windows-x64"
    destination = root / "artifacts" / "CodexWeekly-windows-x64"
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True, exist_ok=True)
    executable = root / "target" / args.target / "release" / "codex-weekly-overlay.exe"
    shutil.copy2(executable, destination / "CodexWeekly.exe")
    shutil.copy2(root / "README.md", destination / "README.md")
    if args.target.endswith("gnu"):
        import os
        cargo_home = pathlib.Path(os.environ.get("CARGO_HOME", str(pathlib.Path.home() / ".cargo")))
        lock = tomllib.loads((root / "Cargo.lock").read_text())
        loader_version = next(package["version"] for package in lock["package"] if package["name"] == "webview2-com-sys")
        candidates = list((cargo_home / "registry" / "src").glob(f"*/webview2-com-sys-{loader_version}/x64/WebView2Loader.dll"))
        if not candidates:
            raise RuntimeError("找不到 GNU 构建需要的 WebView2Loader.dll；请先完成 cargo build。")
        shutil.copy2(candidates[0], destination / "WebView2Loader.dll")
        objdump = shutil.which("x86_64-w64-mingw32-objdump")
        compiler = shutil.which("x86_64-w64-mingw32-gcc")
        if not objdump or not compiler:
            raise RuntimeError("组装 GNU 包需要 MinGW objdump 与 gcc。")
        runtimes = {"libgcc_s_seh-1.dll", "libwinpthread-1.dll", "libstdc++-6.dll"}
        pending = [destination / "CodexWeekly.exe", destination / "WebView2Loader.dll"]
        copied = set()
        while pending:
            output = subprocess.check_output([objdump, "-p", str(pending.pop())], text=True)
            for name in re.findall(r"DLL Name:\s*(\S+)", output):
                if name.lower() not in runtimes or name.lower() in copied:
                    continue
                source = pathlib.Path(subprocess.check_output([compiler, f"-print-file-name={name}"], text=True).strip())
                if not source.is_file():
                    raise RuntimeError(f"找不到运行库 {name}，不能生成不完整的 Windows 包。")
                target = destination / name
                shutil.copy2(source, target)
                copied.add(name.lower())
                pending.append(target)
    archive = destination.parent / f"{release_name}.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as package:
        for path in sorted(destination.rglob('*')):
            if path.is_file():
                package.write(path, f"{destination.name}/{path.relative_to(destination).as_posix()}")
    checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix(".zip.sha256").write_text(f"{checksum}  {archive.name}\n", encoding="utf-8")
    # 只清理本脚本生成的发行产物，保留最新四份压缩包和一个可直接使用的目录。
    releases = []
    for path in destination.parent.iterdir():
        match = re.fullmatch(r"CodexWeekly-(\d+(?:\.\d+)+)(?:-windows-x64)?(?:\.zip)?", path.name)
        if not match:
            continue
        if path.is_dir():
            shutil.rmtree(path)
        elif path.suffix == ".zip":
            releases.append((tuple(map(int, match[1].split("."))), path))
    for _, path in sorted(releases, reverse=True)[4:]:
        path.unlink()
        path.with_suffix(".zip.sha256").unlink(missing_ok=True)
    print(f"已生成：{archive}")


if __name__ == "__main__":
    main()

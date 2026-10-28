# Codex Weekly

薄荷绿的 Codex 周额度悬浮窗。Windows x64 桌面程序，Rust + Tauri 2 + React + SVG；网页界面使用系统 WebView2，无 Electron、C# 或 WinUI/WPF。

## 使用

解压 Windows 可移植包，保留同目录的所有 DLL，双击 `CodexWeekly.exe`。首次启动会显示面板与圆环，后续启动优先恢复悬浮窗。软件单实例运行，再次启动会打开已有面板。

运行条件：Windows 10/11 x64、Microsoft Edge WebView2 Runtime，以及已经登录 Codex 的 WSL 发行版。该 WSL 需要 Python 3（仅标准库）和支持 `codex app-server` 的 Codex CLI。可移植包不自动安装系统组件；若缺少 WebView2，请先安装 [Microsoft 官方运行时](https://developer.microsoft.com/microsoft-edge/webview2/)。Windows 原生构建的 NSIS 安装包包含 WebView2 安装引导。

- **拖动**：解锁时拖动圆环调整位置，位置自动保存。
- **锁定**：右键托盘，勾选“锁定悬浮窗（鼠标穿透）”。整个悬浮窗口会穿透鼠标且不可聚焦；从托盘取消勾选即可恢复拖动。
- **圆环方向与右键**：额度消耗从圆环 12 点沿右上方顺时针扩展；悬浮窗内屏蔽网页右键菜单，管理操作使用托盘。
- **历史**：左键单击托盘打开面板，选择 24 小时、7 天或 30 天；鼠标悬停或用键盘查看采样值。
- **设置**：选择现有 WSL 发行版（默认使用系统默认发行版），设置 30～3600 秒刷新间隔并保存。默认每 60 秒采样。
- **关闭**：关闭主面板释放该 WebView，圆环与托盘继续运行；右键托盘选择“退出”才结束程序。

## 额度和历史

周额度按服务返回的 10080 分钟窗口识别，兼容出现在 `primary` 或 `secondary` 的账号；显示 `100 - usedPercent`。请求只读取账户额度，不创建会话、不发送模型请求，也不兑换额度重置券。窗口重置时间以服务返回为准。

历史从本软件首次成功采样开始积累，没有过去的数据就显示空状态；后台退出期间不会采样。曲线使用 D3 单调平滑插值经过真实采样值，横轴自动适配所选范围内已有记录的时间跨度，避免刚开始记录时所有数据挤成一个点；只有一次采样时会明确提示等待下一次同步。额度重置、已知连接失败、发行版/采样间隔变更、重启和长时间采样缺口均分段记录；曲线不会跨这些边界连线。30 天曲线按首尾与极值降采样，最多 1200 点。本地保留 90 天记录，不生成预测值或虚假的历史。

连接失败会显示上次成功读数和离线状态；没有有效读数时显示占位。即使重置时间已过，也等待服务确认，不擅自显示 100%。切换 Codex 账号后按不可逆账号哈希隔离历史；WSL 原有账号登录变更会在下一次采集生效。

应用数据位于 `%APPDATA%\dev.lingsamuel.codexweeklyoverlay\weekly.sqlite3`，包含额度、时间、账号分组哈希和设置。OAuth 凭证只由原有 WSL Codex 管理，不复制到 Windows，不写入前端或数据库。应用退出后，临时桥接与 app-server 进程组会回收。

## 故障恢复

1. 先在目标 WSL 中确认 `codex --version` 可用，并且 Codex 自身能够显示订阅额度。Windows 桥接会在 WSL 内读取当前用户 shell 的 PATH、代理、NVM 和 CODEX_HOME 白名单，兼容交互终端才加载的配置；其余环境变量不导入。程序也支持常见 NVM Node 安装和 `~/.local/bin` / `~/.cargo/bin` 中的 Codex。
2. 在面板中选择正确的 WSL 发行版，保存并刷新。WSL 中的 `python3 --version` 必须可运行。
3. 登录过期时，在该 WSL 中恢复原有 Codex 登录，再点击刷新；本程序不会弹出第二套登录流程。
4. 超时请检查 WSL 网络。每次桥接有超时，失败不会清空上次有效读数；本地保存失败时检查磁盘空间与应用数据目录权限。

可用 `CodexWeekly.exe --diagnose C:\path\diagnostics.json` 仅验证 Windows→WSL→Codex 的只读连接并退出。报告包含版本、额度和错误说明，不含账户 ID 或凭证。

## 开发和验证

Node.js 22.18+、npm、当前稳定版 Rust（本次验证为 1.99.1）、Python 3.11+。以锁文件中的依赖版本为准。Rust 核心库与 Windows UI 分离，可以直接在 WSL 测试：

```sh
npm ci
npm run check
npm test
npm run build
cargo test -p limit-core --locked
cargo clippy -p limit-core -- -D warnings
python3 -m unittest discover -s scripts -p 'test_*.py'
```

网页预览：`npm run dev` 后访问 `http://127.0.0.1:1420/?demo=1`；悬浮窗预览用 `?view=overlay&demo=1`。只有显式 `demo=1` 使用演示数据；普通浏览器无法访问本地额度，不会自动生成假数据。

Windows 桌面开发（安装 Visual Studio C++ 工具、Rust MSVC 和 WebView2）：`npm run tauri -- dev`。`custom-protocol` 特性用于发布内嵌网页资源，由 Tauri CLI 的 build 或本仓库发布脚本启用。直接使用 Cargo 发布构建时必须显式加 `--features custom-protocol`；开发服务器不是发布依赖。

WSL 交叉编译：安装 MinGW-w64 GCC、binutils 与 Windows GNU Rust 目标，然后运行 `bash scripts/build-windows.sh`。输出位于 `artifacts/`，包含完整可移植目录、ZIP 和 SHA-256；GNU 版自动包含锁定版本的 WebView2Loader 与实际导入的 MinGW DLL。

Windows MSVC 构建：运行 `powershell -ExecutionPolicy Bypass -File scripts/build-windows.ps1`。构建工具需要联网获取首次安装的依赖与 NSIS 工具。仓库不要求用户重新登录 Codex，也不提供 WSL/Codex 的自动安装功能。

## 资源使用

圆环采用事件驱动 SVG，没有持续动画或逐帧循环；静态界面使用软件合成以减少空闲 GPU 资源。主面板关闭后销毁，后台串行采集，临时 Codex app-server 采集结束即退出。每 10 秒检查一次显示器位置，避免外接屏移除后悬浮窗遗失。内存包含共享 WebView2 运行时进程，不能仅用 exe 内存代表整个软件；实际测量与验证范围记录在 `docs/verification.md`。

## 协议参考

- [OpenAI Codex App Server](https://learn.chatgpt.com/docs/app-server)
- [Tauri 托盘](https://v2.tauri.app/learn/system-tray/)
- [Tauri 窗口 API](https://v2.tauri.app/reference/javascript/api/namespacewebviewwindow/)

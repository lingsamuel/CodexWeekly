# Codex Weekly

薄荷绿的 Codex 周额度悬浮窗。Windows x64 桌面程序，Rust + Tauri 2 + React + SVG；网页界面使用系统 WebView2，无 Electron、C# 或 WinUI/WPF。

## 使用

解压 Windows 可移植包，保留同目录的所有 DLL，双击 `CodexWeekly.exe`。首次启动会显示面板与圆环，后续启动优先恢复悬浮窗。软件单实例运行，再次启动会打开已有面板。

运行条件：Windows 10/11 x64、Microsoft Edge WebView2 Runtime，以及已经登录 Codex 的 WSL 发行版。该 WSL 需要 Python 3（仅标准库）和支持 `codex app-server` 的 Codex CLI。可移植包不自动安装系统组件；若缺少 WebView2，请先安装 [Microsoft 官方运行时](https://developer.microsoft.com/microsoft-edge/webview2/)。Windows 原生构建的 NSIS 安装包包含 WebView2 安装引导。

- **拖动**：解锁时拖动圆环调整位置，位置自动保存。
- **息屏恢复**：亮屏、唤醒或显示器/DPI 变化后自动恢复完整圆环与用户位置；系统临时移位不覆盖拖动位置。系统意外最小化时会恢复悬浮窗，不抢焦点；从托盘隐藏时继续保持隐藏。副屏断开时临时回到主屏，重连后恢复原位置。
- **锁定**：右键托盘，勾选“锁定悬浮窗（鼠标穿透）”。锁定后圆环和空白区域允许鼠标操作下方窗口，悬浮窗保持不可聚焦。气泡可悬停和拖动，详情列表可滚动。从托盘取消勾选后也可拖动圆环。
- **圆环方向与右键**：额度消耗从圆环 12 点沿右上方顺时针扩展；悬浮窗内屏蔽网页右键菜单，管理操作使用托盘。
- **重置倒计时**：悬浮圆环下方显示“X天Y小时后重置”，按剩余完整小时计算，每分钟更新；时间缺失或已过期时显示等待提示。
- **历史**：左键单击托盘打开面板，选择 24 小时、7 天或 30 天；鼠标悬停或用键盘查看采样值。
- **托盘余量**：托盘图标复用悬浮圆环，中心显示整数余量；悬停可查看完整百分比。图标只在额度显示值或离线状态变化时重新生成。
- **会话气泡**：右上蓝色表示工作中的顶层交互会话，右下红色只统计本次程序启动后完成、且回执仍未读的会话；启动前的历史积压不计入。0 个或状态不可用时隐藏，超过 99 显示 `99+`。
- **完成提示音**：检测到新的红色气泡完成项时，后台播放内置的短提示音。每批刷新最多响一声，同一会话的同一轮只提醒一次，悬浮窗隐藏时也有效。默认开启，右键托盘可切换“任务完成提示音”，设置会保存。关闭期间已检测到的完成项，重新开启后不会补响。刷新前就已被标为已读的结果不会触发声音。
- **会话详情**：悬停气泡会动画展开列表，按 Project 展示会话标题。两种状态同时展开时，同一 Project 共用一个标题，条目用蓝色和红色标出状态。鼠标移入列表可继续查看，按住任一气泡的左键可拖动整个悬浮窗。刷新检测到新增工作中会话或未读结果时，对应列表自动展开 8 秒；继续有新增时重新计时，悬停时保持展开。
- **设置**：选择现有 WSL 发行版（默认使用系统默认发行版），设置 30～3600 秒刷新间隔并保存。默认每 60 秒采样。
- **关闭**：关闭主面板释放该 WebView，圆环与托盘继续运行；右键托盘选择“退出”才结束程序。

## 额度和历史

周额度按服务返回的 10080 分钟窗口识别，兼容出现在 `primary` 或 `secondary` 的账号；显示 `100 - usedPercent`。请求只读取账户额度，不创建会话、不发送模型请求，也不兑换额度重置券。窗口重置时间以服务返回为准。

历史从本软件首次成功采样开始积累，没有过去的数据就显示空状态；后台退出期间不会采样。当前账号在所选范围内的采样点按时间排序，使用 D3 B 样条平滑拟合绘制为一条连续实线，底部渐变面积同样连续；采集分段不再造成图中的断口、孤立点或虚线。横轴自动适配已有记录跨度，只有一次采样时提示等待下一次同步。30 天曲线按首尾与极值降采样，最多 1200 点；悬停值来自真实采样，插值不写入数据库。本地保留 90 天记录。

连接失败会显示上次成功读数和离线状态；没有有效读数时显示占位。即使重置时间已过，也等待服务确认，不擅自显示 100%。切换 Codex 账号后按不可逆账号哈希隔离历史；WSL 原有账号登录变更会在下一次采集生效。

应用数据位于 `%APPDATA%\dev.lingsamuel.codexweeklyoverlay\weekly.sqlite3`，包含额度、时间、账号分组哈希和设置。OAuth 凭证只由原有 WSL Codex 管理，不复制到 Windows，不写入前端或数据库。应用退出后，临时桥接与 app-server 进程组会回收。

## 故障恢复

1. 先在目标 WSL 中确认 `codex --version` 可用，并且 Codex 自身能够显示订阅额度。Windows 桥接会在 WSL 内读取当前用户 shell 的 PATH、代理、NVM 和 CODEX_HOME 白名单，兼容交互终端才加载的配置；其余环境变量不导入。程序也支持常见 NVM Node 安装和 `~/.local/bin` / `~/.cargo/bin` 中的 Codex。
2. 在面板中选择正确的 WSL 发行版，保存并刷新。WSL 中的 `python3 --version` 必须可运行。
3. 登录过期时，在该 WSL 中恢复原有 Codex 登录，再点击刷新；本程序不会弹出第二套登录流程。
4. 超时请检查 WSL 网络。每次桥接有超时，失败不会清空上次有效读数；本地保存失败时检查磁盘空间与应用数据目录权限。

可用 `CodexWeekly.exe --diagnose C:\path\diagnostics.json` 仅验证 Windows→WSL→Codex 的只读连接并退出。报告包含版本、额度和错误说明，不含账户 ID 或凭证。

## 会话状态来源

气泡使用所选 WSL 发行版中**现有 Codex daemon** 的真实状态，每 15 秒更新。通过原生 `codex app-server proxy` 和 WebSocket 控制连接读取完整分页会话列表；只统计 CLI、VS Code 与 app-server 的顶层交互会话，不包含子代理或已归档会话。蓝色采用 `active` 状态（包括等待确认）；红色须同时具备未读回执、最后一轮为 `completed`，且该轮真实 `completedAt` 不早于本次程序启动时刻。旧会话在启动后完成新一轮也计入，启动前的历史结果不计入；失败、中断或仍在生成的轮次不会冒充已完成。

启动时刻在程序入口固定，断线重连、刷新和发行版切换都不重置，退出再启动才重新划定范围。完成时间缺失或无效时隐藏未知计数，不用会话的创建/更新时间猜测。红色表示“本次启动后完成且回执未读”；是否清除回执仍取决于原客户端，不能等同于确认用户实际没有阅读。

当前本机 0.164/0.165 协议已确认提供 `thread/list.readStates` 和最后轮次查询。不会调用标记已读、启动或恢复会话的写接口，也不建立额外网络监听服务；没有 daemon 或接口不可用时隐藏气泡，额度圆环继续独立工作。非此 daemon 管理的其他 Codex 进程或云端状态不在计数范围内。

详情复用会话列表的标题和项目归属，项目名称通过 `project/list` 只读查询。未分配 Project 的会话按工作目录归组，目录缺失时归入“未归属”。缺少标题时显示“未命名会话”。详情数据只在本次运行中使用。

可用 `CodexWeekly.exe --diagnose-sessions C:\path\sessions.json` 在不打开界面的情况下验证会话统计。诊断进程也以自己的启动时间为界；报告只含计数、版本、启动时刻与错误说明，不含会话标题、预览、标识或内容。

## 开发和验证

Node.js 22.18+、npm、当前稳定版 Rust（本次验证为 1.99.1）、Python 3.11+。以锁文件中的依赖版本为准。Rust 核心库与 Windows UI 分离，可以直接在 WSL 测试：

```sh
npm ci
npm run check
npm test
npm run build
cargo test -p limit-core --locked
cargo test -p codex-weekly-overlay --test overlay_geometry --locked
cargo clippy -p limit-core -- -D warnings
python3 -m unittest discover -s scripts -p 'test_*.py'
```

网页预览：`npm run dev` 后访问 `http://127.0.0.1:1420/?demo=1`；悬浮窗预览用 `?view=overlay&demo=1`。只有显式 `demo=1` 使用演示数据；普通浏览器无法访问本地额度，不会自动生成假数据。

Windows 桌面开发（安装 Visual Studio C++ 工具、Rust MSVC 和 WebView2）：`npm run tauri -- dev`。`custom-protocol` 特性用于发布内嵌网页资源，由 Tauri CLI 的 build 或本仓库发布脚本启用。直接使用 Cargo 发布构建时必须显式加 `--features custom-protocol`；开发服务器不是发布依赖。

WSL 交叉编译：安装 MinGW-w64 GCC、binutils 与 Windows GNU Rust 目标，然后运行 `bash scripts/build-windows.sh`。输出位于 `artifacts/`，包含完整可移植目录、ZIP 和 SHA-256；GNU 版自动包含锁定版本的 WebView2Loader 与实际导入的 MinGW DLL。

当前代码已经通过相应测试时，可使用 `bash scripts/build-windows.sh --skip-tests` 避免重复测试。可直接运行的目录固定为 `artifacts/CodexWeekly-windows-x64`，压缩包包含版本号；最多保留最新四份压缩包，不将 `docs` 开发文档打入发布包。

Windows MSVC 构建：运行 `powershell -ExecutionPolicy Bypass -File scripts/build-windows.ps1`。构建工具需要联网获取首次安装的依赖与 NSIS 工具。仓库不要求用户重新登录 Codex，也不提供 WSL/Codex 的自动安装功能。

## 资源使用

圆环采用事件驱动 SVG，没有持续动画或逐帧循环；静态界面使用软件合成以减少空闲 GPU 资源。主面板关闭后销毁，后台串行采集，额度查询用的临时 Codex app-server 采集结束即退出。会话统计另维持一条到现有 daemon 的轻量代理连接，并缓存未变化的完成状态；退出仅关闭自己的代理，不停止 daemon。屏幕电源、显示器、工作区和 DPI 通知经 750 毫秒合并后恢复窗口与 WebView 边界，另保留每 10 秒的几何检查；息屏、拖动及位置保存期间暂停兜底恢复。只有用户完成拖动才保存位置。内存包含共享 WebView2 运行时进程，不能仅用 exe 内存代表整个软件；实际测量与验证范围记录在 `docs/verification.md`。

## 协议参考

- [OpenAI Codex App Server](https://learn.chatgpt.com/docs/app-server)
- [Tauri 托盘](https://v2.tauri.app/learn/system-tray/)
- [Tauri 窗口 API](https://v2.tauri.app/reference/javascript/api/namespacewebviewwindow/)

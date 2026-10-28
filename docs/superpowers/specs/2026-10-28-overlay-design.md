# Codex Weekly Overlay 设计

状态：用户已确认方案（2026-10-28）。首发版本：0.1.0。

## 目标和边界

Windows 11 常驻额度悬浮窗，复用 WSL 的 Codex 登录，显示 Weekly 剩余百分比，并记录真实额度变化。采用 Rust、Tauri 2、TypeScript、React、CSS 与 SVG；不使用 C#、WinUI/WPF 或 Electron。WebView2 使用系统共享运行时。

## 视觉与交互

- 薄荷绿与暖白配色，深松绿色文字，柔和渐变、精细边框、留白和清晰的数字排版。
- 透明、无边框、置顶的小型圆环，中间为剩余百分比，辅助文字为 WEEKLY 与更新时间/异常状态。SVG 只在数据变化时更新，无永久循环动画。
- 初始解锁，可以拖动；托盘右键的“锁定悬浮窗”控制整个窗口的鼠标穿透与不可聚焦状态。锁定后仍可从托盘解锁。保存位置和锁定状态；重启及显示器布局变化时避免落在屏幕外。
- 左键单击托盘打开或聚焦主窗口，右键菜单提供打开面板、锁定、显示/隐藏悬浮窗、立即刷新、退出。
- 主面板提供 Weekly 剩余、重置时间、连接状态及 24 小时／7 天／30 天曲线，指针查看采样值；显示首次记录的空状态，不生成虚假历史。额度重置、断线与账号切换不连成误导的连续消耗曲线。
- 关闭主面板销毁其 WebView，托盘和悬浮窗继续运行；退出托盘时终止后台任务。

## 数据连接

Windows Rust 后端使用 `wsl.exe` 在指定发行版执行内嵌的 Python 3 标准库桥接程序。桥接程序定位现有 Codex（包括 NVM 的 Node 路径），启动 `codex app-server`，通过 JSONL 执行 `initialize` → `initialized` → `account/rateLimits/read`，读完后关闭子进程。不复制、输出或向 Windows 暴露 OAuth 凭证，不创建对话或调用模型。

Windows 实测补充：`wsl.exe --exec` 不加载交互 shell，可能同时丢失 NVM PATH 和代理。桥接在 WSL 内以当前用户 shell 获取环境，仅导入 PATH、CODEX_HOME、NVM_DIR 与代理变量白名单；其余变量不导入，也不把整个环境发往 Windows。shell 初始化最多 5 秒。

默认使用 WSL 默认发行版，设置中可选择发行版；默认每 60 秒采样，可设置 30～3600 秒。手动刷新与定时任务共用单一采集入口，避免并行请求。空闲时没有常驻 Codex 子进程；一次采集有明确超时和进程回收。

实测：本机 CLI 0.164.0 可通过已有登录读取周额度；`codex` 桶的 `primary.windowDurationMins = 10080`，`usedPercent = 27`。现有 0.165.0 daemon 的 proxy 握手未在探测期限内响应，因此首版采用已验证的独立 app-server，不依赖 daemon 内部控制协议。

## 额度语义

优先选择 `rateLimitsByLimitId.codex`，兼容旧协议 `rateLimits`。在 `primary` 与 `secondary` 中按 `windowDurationMins == 10080` 选择周窗口，不假定一定是 secondary。`remainingPercent = 100 - usedPercent`；非法、缺失或无法识别周窗口时显示明确错误，不显示为 100%。保留账号标识用于历史隔离，但不向前端公开账号 ID。不提供清空额度或兑换重置的写接口。

## 存储和异常

Windows 应用数据目录的 SQLite 保存采样、账号分组与设置。保留 90 天历史，图表按范围降采样并保留窗口重置分段。只存额度、时间、窗口及必要的账号隔离标识，不存凭证。数据库失败可见，不把未持久化状态报告为完整历史。

分段在采集时持久化，包括服务重置、已知失败后的下一次成功、账号变更（含 A→B→A）、设置变更、重启与超过当时采样间隔三倍的缺口。修改当前刷新间隔不会重解释旧历史。Windows 父管道关闭时 WSL 桥接取消采集并回收整个 app-server 进程组；守护线程使用原始 fd 读取，避免 Python 退出时的缓冲锁冲突。

连接失败保留上次有效读数并标记过期；启动没有历史时显示占位。重置时间过去也不能擅自推断额度变为 100%，等待服务返回。错误按 WSL/Codex 缺失、登录失效、协议不兼容、超时和一般连接失败分类，并提供可执行的恢复说明。

## 验证和交付

核心单元测试覆盖 primary/secondary 周窗口、旧/新协议、错误值、SQLite 存储、账号隔离、历史范围、过期和设置校验。桥接测试使用可控假 app-server 验证 JSONL 握手、超时、错误及清理，并用真实 WSL 凭证做只读集成验证。前端执行 TypeScript 检查、生产构建及浏览器交互/视觉检查。交付 Windows x64 可执行文件及可复现构建说明；实测记录分别说明已编译、已运行及未验证的手工项目。性能按真实进程 CPU/内存记录，不预先承诺未经测量的占用值。

## 官方参考

- https://learn.chatgpt.com/docs/app-server
- https://v2.tauri.app/learn/system-tray/
- https://v2.tauri.app/reference/javascript/api/namespacewebviewwindow/

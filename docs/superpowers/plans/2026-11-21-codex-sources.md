# Codex 采集来源实施计划

Windows 和 WSL 共用账号额度查询与历史曲线。各实例单独保存最后一次采样，并在切换时清理会话和过期请求。

- [x] 在 `limit-core` 验证旧设置兼容、两个实例共享历史和各自读取缓存，以及只读 app-server 握手与账号 ID 回退。用 `cargo test -p limit-core --locked` 验证。保留原历史表与账号哈希，在同一事务中写入历史和来源缓存。
- [x] 在 `scripts/wsl_bridge.py` 保留 Linux 环境恢复和进程组清理，将标准输入输出接给 Rust 的额度查询。用 `python3 -m unittest discover -s scripts -p 'test_*.py'` 验证父管道关闭及进程组回收。
- [x] 在 `src-tauri` 增加 Windows Codex 可执行文件查找、实际 WSL 发行版解析和诊断入口。同步实例切换序号，串行采样及取消上一个实例的请求，复用会话代理和提示音判断。
- [x] 在 React 面板增加来源选择，以账号曲线序号检查异步读数。覆盖无 WSL 时选择 Windows 和切换后迟到的历史响应，沿用浏览器回归入口。
- [x] 同步版本、说明和验收文档，运行前端、Python、Rust 测试及 Windows 目标检查。用受版本控制的脚本构建可移植包，运行 Windows 和 WSL 诊断，核对结果后提交。

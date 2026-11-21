# 悬浮窗周额度节奏实施计划

- [x] 扩展 `src/lib/format.test.ts`，覆盖 `quotaPace` 的 ±5 个百分点边界、缺失时间和窗口到期。在 `src/lib/format.ts` 复用剩余时间计算，并返回节奏文案与已过比例。
- [x] 扩展 `src/tests/browser-check.mjs`，通过已有快照 fixture 检查七日刻度、指针、同步失败配色和恢复。用 `QuotaRing` 绘制刻度和指针，在 `Overlay` 依据同步状态选择底部文案。保持托盘 SVG 序列化仅包含额度图形。
- [x] 同步 README 和版本元数据。运行锁定依赖的可移植包构建及浏览器回归，检查实际悬浮窗截图，记录验证并一次提交。

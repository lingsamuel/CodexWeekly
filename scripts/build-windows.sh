#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
skip_tests=false
case "${1:-}" in
  '') ;;
  --skip-tests) skip_tests=true ;;
  *) echo "用法：$0 [--skip-tests]（仅在当前代码已验证时跳过测试）" >&2; exit 1 ;;
esac
for tool in npm cargo rustup python3 x86_64-w64-mingw32-gcc x86_64-w64-mingw32-windres; do
  command -v "$tool" >/dev/null || { echo "缺少构建工具：$tool" >&2; exit 1; }
done
rustup target add x86_64-pc-windows-gnu
npm ci
if ! "$skip_tests"; then
  npm test
  python3 -m unittest discover -s scripts -p 'test_*.py'
  cargo test -p limit-core --locked
  cargo test -p codex-weekly-overlay --test overlay_geometry --locked
fi
npm run build
cargo build -p codex-weekly-overlay --release --features custom-protocol --target x86_64-pc-windows-gnu --locked
python3 scripts/package_windows.py

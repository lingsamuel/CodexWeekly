$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

function Invoke-Checked {
    param([string]$Command, [string[]]$Arguments)
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed with exit code $LASTEXITCODE" }
}

Invoke-Checked 'npm.cmd' @('ci')
Invoke-Checked 'npm.cmd' @('test')
Invoke-Checked 'cargo' @('test', '-p', 'limit-core', '--locked')
Invoke-Checked 'cargo' @('test', '-p', 'codex-weekly-overlay', '--test', 'overlay_geometry', '--locked')
# Tauri CLI 在 Windows MSVC 上生成包含 WebView2 安装引导的 NSIS 安装包。
Invoke-Checked 'npm.cmd' @('run', 'tauri', '--', 'build', '--target', 'x86_64-pc-windows-msvc')
Write-Host 'Windows 安装包已生成于 target/x86_64-pc-windows-msvc/release/bundle/nsis。'

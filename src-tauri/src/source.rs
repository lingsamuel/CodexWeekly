use limit_core::settings::{Settings, SourceKind};
use std::{
    env,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
    time::Duration,
};
use tokio::process::Command;
use winreg::{enums::HKEY_CURRENT_USER, RegKey};

const BRIDGE: &str = include_str!("../../scripts/wsl_bridge.py");
const CREATE_NO_WINDOW: u32 = 0x08000000;
const WSL_REGISTRY: &str = r"Software\Microsoft\Windows\CurrentVersion\Lxss";
static DESKTOP_CODEX: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CodexSource {
    Windows,
    Wsl { id: String, distro: String },
}

#[derive(Clone, Copy)]
pub enum AppServerMode {
    Quota,
    DaemonProxy,
}

impl CodexSource {
    pub fn resolve(settings: &Settings) -> Result<Self, String> {
        if settings.source == SourceKind::Windows {
            return Ok(Self::Windows);
        }
        let registry = wsl_registry()?.ok_or("Windows 未安装 WSL 发行版。")?;
        let entries = registered_distros(&registry);
        let selected = if settings.distro.is_empty() {
            let default: String = registry
                .get_value("DefaultDistribution")
                .map_err(|_| "WSL 尚未设置默认发行版。")?;
            entries
                .into_iter()
                .find(|(id, _)| id.eq_ignore_ascii_case(&default))
        } else {
            entries
                .iter()
                .find(|(_, distro)| distro == &settings.distro)
                .or_else(|| {
                    entries
                        .iter()
                        .find(|(_, distro)| distro.eq_ignore_ascii_case(&settings.distro))
                })
                .cloned()
        };
        let (id, distro) = selected.ok_or("找不到所选的 WSL 发行版。")?;
        Ok(Self::Wsl { id, distro })
    }

    pub fn cache_key(&self) -> &str {
        // 已注册发行版的 ID 同时适用于默认项和手动选择，也能区分重新注册的同名实例。
        match self {
            Self::Windows => "windows",
            Self::Wsl { id, .. } => id,
        }
    }

    pub async fn app_server(&self, mode: AppServerMode) -> Result<Command, String> {
        let mut command = match self {
            Self::Windows => {
                let mut command = hidden_command(windows_codex().await?);
                if let Some(profile) = env::var_os("USERPROFILE")
                    .map(PathBuf::from)
                    .filter(|path| path.is_dir())
                {
                    command.current_dir(profile);
                }
                command.arg("app-server");
                match mode {
                    AppServerMode::Quota => command.args(["--listen", "stdio://"]),
                    AppServerMode::DaemonProxy => command.arg("proxy"),
                };
                command
            }
            Self::Wsl { distro, .. } => {
                let mut command = hidden_command("wsl.exe");
                command.args([
                    "--distribution",
                    distro,
                    "--cd",
                    "~",
                    "--exec",
                    "python3",
                    "-u",
                    "-c",
                    BRIDGE,
                    "--restore-shell-environment",
                ]);
                if matches!(mode, AppServerMode::DaemonProxy) {
                    command.arg("--daemon-proxy");
                }
                command
            }
        };
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        Ok(command)
    }

    pub fn launch_error(&self) -> &'static str {
        match self {
            Self::Windows => "无法启动 Windows 本机 Codex，请检查 Codex 安装。",
            Self::Wsl { .. } => "无法启动 WSL Codex，请检查所选发行版与 Python 3。",
        }
    }
}

pub fn list_distros() -> Result<Vec<String>, String> {
    let Some(registry) = wsl_registry()? else {
        return Ok(Vec::new());
    };
    let mut names: Vec<_> = registered_distros(&registry)
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    names.sort_unstable();
    Ok(names)
}

fn wsl_registry() -> Result<Option<RegKey>, String> {
    match RegKey::predef(HKEY_CURRENT_USER).open_subkey(WSL_REGISTRY) {
        Ok(key) => Ok(Some(key)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(_) => Err("无法读取 Windows 上的 WSL 发行版设置。".into()),
    }
}

fn registered_distros(registry: &RegKey) -> Vec<(String, String)> {
    registry
        .enum_keys()
        .filter_map(Result::ok)
        .filter_map(|id| {
            let key = registry.open_subkey(&id).ok()?;
            Some((id, key.get_value("DistributionName").ok()?))
        })
        .collect()
}

fn hidden_command(program: impl AsRef<Path>) -> Command {
    let mut command = Command::new(program.as_ref());
    command.creation_flags(CREATE_NO_WINDOW).kill_on_drop(true);
    command
}

async fn windows_codex() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path).filter(|path| path.is_absolute()) {
            let executable = directory.join("codex.exe");
            if executable.is_file() {
                return Ok(executable);
            }
            if directory.join("codex.cmd").is_file() {
                let npm_root = directory.join(r"node_modules\@openai");
                // npm 启动脚本最终执行平台包的 EXE。直接启动它，以免退出时只关闭 cmd.exe。
                for root in [npm_root.join(r"codex\node_modules\@openai"), npm_root] {
                    let executable =
                        root.join(r"codex-win32-x64\vendor\x86_64-pc-windows-msvc\bin\codex.exe");
                    if executable.is_file() {
                        return Ok(executable);
                    }
                }
            }
        }
    }
    desktop_codex()
        .await
        .ok_or_else(|| "未找到 Windows 本机 Codex。请安装 Codex CLI 或官方桌面版。".into())
}

async fn desktop_codex() -> Option<PathBuf> {
    if let Some(path) = DESKTOP_CODEX
        .lock()
        .ok()?
        .as_ref()
        .filter(|path| path.is_file())
        .cloned()
    {
        return Some(path);
    }
    // 由 Windows 查询当前用户已安装的官方桌面包，更新后会自动使用新安装目录。
    let powershell = PathBuf::from(env::var_os("SystemRoot")?)
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let output = tokio::time::timeout(Duration::from_secs(5), hidden_command(powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", r"[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); Get-AppxPackage -Name OpenAI.Codex | ForEach-Object { Join-Path $_.InstallLocation 'app\resources\codex.exe' }"])
        .stderr(Stdio::null()).output()).await.ok()?.ok()?;
    let path = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .map(PathBuf::from)
        .find(|path| path.is_absolute() && path.is_file())?;
    *DESKTOP_CODEX.lock().ok()? = Some(path.clone());
    Some(path)
}

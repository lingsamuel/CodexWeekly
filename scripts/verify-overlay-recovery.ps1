param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [int]$DebugPort = 9238
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$Executable = (Resolve-Path $Executable).Path
# 单实例 mutex 跨桌面共享，必须使用独立 identifier 构建，防止触发用户实例的聚焦回调。
if (-not [Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($Executable)).Contains('dev.lingsamuel.codexweeklyrecoverytest')) {
    throw '只允许运行 identifier=dev.lingsamuel.codexweeklyrecoverytest 的隔离测试构建。'
}
[void][IO.Directory]::CreateDirectory($OutputDirectory)
$reportPath = Join-Path $OutputDirectory 'native-recovery.json'
if (Test-Path $reportPath) { Remove-Item $reportPath }
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class RecoveryProbe {
    public delegate bool EnumProc(IntPtr hwnd, IntPtr param);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct Startup {
        public int Size; public string Reserved,Desktop,Title; public int X,Y,Width,Height,CharsX,CharsY,Fill,Flags;
        public short Show,ReservedSize; public IntPtr ReservedData,Input,Output,Error;
    }
    [StructLayout(LayoutKind.Sequential)] public struct ProcessInfo { public IntPtr Process,Thread; public int Id,ThreadId; }
    [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)] public static extern IntPtr CreateDesktop(string name,IntPtr device,IntPtr mode,int flags,uint access,IntPtr security);
    [DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr desktop);
    [DllImport("user32.dll")] public static extern bool EnumDesktopWindows(IntPtr desktop,EnumProc callback,IntPtr param);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint process);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder text,int size);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd,StringBuilder text,int size);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd,IntPtr after,int x,int y,int width,int height,uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd,uint message,IntPtr wparam,IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd,uint message,IntPtr wparam,IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd,int command);
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] public static extern bool CreateProcess(string app,StringBuilder command,IntPtr ps,IntPtr ts,bool inherit,uint flags,IntPtr environment,string cwd,ref Startup startup,out ProcessInfo info);
    [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll")] public static extern bool TerminateProcess(IntPtr process,uint code);
    public static IntPtr FindOverlay(IntPtr desktop,int processId) {
        return FindWindow(desktop,processId,"Codex Weekly Overlay",null);
    }
    public static IntPtr FindTray(IntPtr desktop,int processId) {
        return FindWindow(desktop,processId,null,"tray_icon_app");
    }
    private static IntPtr FindWindow(IntPtr desktop,int processId,string title,string className) {
        IntPtr result=IntPtr.Zero;
        EnumDesktopWindows(desktop,(hwnd,unused)=>{
            uint owner;GetWindowThreadProcessId(hwnd,out owner);
            var text=new StringBuilder(256);GetWindowText(hwnd,text,256);
            var cls=new StringBuilder(256);GetClassName(hwnd,cls,256);
            if(owner==processId && (title==null || text.ToString()==title) && (className==null || cls.ToString()==className)) result=hwnd;
            return true;
        },IntPtr.Zero);
        return result;
    }
}
'@
[void][RecoveryProbe]::SetProcessDpiAwarenessContext([IntPtr](-4))
$desktopName = "CodexWeeklyRecovery-$([Guid]::NewGuid().ToString('N'))"
$desktop = [RecoveryProbe]::CreateDesktop($desktopName,[IntPtr]::Zero,[IntPtr]::Zero,0,0x01ff,[IntPtr]::Zero)
if ($desktop -eq [IntPtr]::Zero) { throw '无法创建隔离桌面。' }
$info = [RecoveryProbe+ProcessInfo]::new()
$previousArgs = $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS

function Read-Rect([IntPtr]$Window) {
    $rect = [RecoveryProbe+Rect]::new()
    if (-not [RecoveryProbe]::GetWindowRect($Window,[ref]$rect)) { throw '无法读取窗口位置。' }
    return @{X=$rect.Left;Y=$rect.Top;Width=$rect.Right-$rect.Left;Height=$rect.Bottom-$rect.Top}
}
function Assert-Restored([IntPtr]$Window,$Expected,[string]$Name) {
    $deadline = [DateTime]::UtcNow.AddSeconds(6)
    do {
        Start-Sleep -Milliseconds 100
        $actual = Read-Rect $Window
        if (-not [RecoveryProbe]::IsIconic($Window) -and $actual.X -eq $Expected.X -and $actual.Y -eq $Expected.Y -and $actual.Width -eq $Expected.Width -and $actual.Height -eq $Expected.Height) {
            return @{Name=$Name;Rect=$actual;Passed=$true}
        }
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "$Name 未恢复：$($actual | ConvertTo-Json -Compress)，预期 $($Expected | ConvertTo-Json -Compress)"
}
function Send-DisplayPower([IntPtr]$Window,[int]$State) {
    $data = [Runtime.InteropServices.Marshal]::AllocHGlobal(24)
    try {
        $guid = [Guid]'6fe69556-704a-47a0-8f24-c28d936fda47'
        [Runtime.InteropServices.Marshal]::Copy($guid.ToByteArray(),0,$data,16)
        [Runtime.InteropServices.Marshal]::WriteInt32($data,16,4)
        [Runtime.InteropServices.Marshal]::WriteInt32($data,20,$State)
        [void][RecoveryProbe]::SendMessage($Window,0x218,[IntPtr]0x8013,$data)
    } finally { [Runtime.InteropServices.Marshal]::FreeHGlobal($data) }
}
try {
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$DebugPort"
    $startup = [RecoveryProbe+Startup]::new()
    $startup.Size = [Runtime.InteropServices.Marshal]::SizeOf($startup)
    # 不调用 SwitchDesktop；测试窗口、输入与用户桌面完全隔离。
    $startup.Desktop = "winsta0\$desktopName"
    if (-not [RecoveryProbe]::CreateProcess($Executable,[Text.StringBuilder]::new('"'+$Executable+'"'),[IntPtr]::Zero,[IntPtr]::Zero,$false,0,[IntPtr]::Zero,(Split-Path $Executable),[ref]$startup,[ref]$info)) {
        throw "无法启动测试程序：$([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $window = [IntPtr]::Zero
    for ($i=0;$i -lt 150 -and $window -eq [IntPtr]::Zero;$i++) {
        Start-Sleep -Milliseconds 100
        $window = [RecoveryProbe]::FindOverlay($desktop,$info.Id)
    }
    if ($window -eq [IntPtr]::Zero) { throw '未找到隔离测试悬浮窗。' }
    Start-Sleep -Seconds 2
    $initial = Read-Rect $window
    @{Phase='ready';ProcessId=$info.Id;Rect=$initial;DebugPort=$DebugPort} | ConvertTo-Json -Compress
    Start-Sleep -Seconds 3
    $results = @()
    # 模拟驱动重连导致 HWND 移位并压扁，不修改真实屏幕电源、DPI 或用户输入。
    [void][RecoveryProbe]::SendMessage($window,0x218,[IntPtr]4,[IntPtr]::Zero) # PBT_APMSUSPEND
    [void][RecoveryProbe]::SetWindowPos($window,[IntPtr]::Zero,80,90,$initial.Width,22,0x0014)
    [void][RecoveryProbe]::SendMessage($window,0x218,[IntPtr]18,[IntPtr]::Zero) # PBT_APMRESUMEAUTOMATIC
    $results += Assert-Restored $window $initial 'resume'
    # 真实故障的 HWND 仍 visible，但 iconic 且坐标为 -32000；位置校正不能解除最小化。
    $foreground = [RecoveryProbe]::GetForegroundWindow()
    Send-DisplayPower $window 0
    [void][RecoveryProbe]::ShowWindow($window,7) # SW_SHOWMINNOACTIVE
    if (-not [RecoveryProbe]::IsIconic($window)) { throw '未复现最小化状态。' }
    Send-DisplayPower $window 1
    $results += Assert-Restored $window $initial 'minimized-screen-off-on'
    if ([RecoveryProbe]::GetForegroundWindow() -ne $foreground) { throw '最小化恢复改变了前台窗口。' }
    [void][RecoveryProbe]::ShowWindow($window,7)
    $results += Assert-Restored $window $initial 'minimized-without-power-event'
    Send-DisplayPower $window 0
    [void][RecoveryProbe]::SendMessage($window,0x231,[IntPtr]::Zero,[IntPtr]::Zero)
    [void][RecoveryProbe]::ShowWindow($window,7)
    [void][RecoveryProbe]::SendMessage($window,0x232,[IntPtr]::Zero,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 500
    Send-DisplayPower $window 1
    $results += Assert-Restored $window $initial 'minimized-interrupts-drag'
    # 真正通过应用托盘命令切换保存的显示偏好，覆盖隐藏期间仍处于最小化的状态。
    $tray = [RecoveryProbe]::FindTray($desktop,$info.Id)
    if ($tray -eq [IntPtr]::Zero) { throw '未找到隔离测试托盘。' }
    Send-DisplayPower $window 0
    [void][RecoveryProbe]::ShowWindow($window,7)
    [void][RecoveryProbe]::PostMessage($tray,0x111,[IntPtr]1002,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 500
    if ([RecoveryProbe]::IsWindowVisible($window)) { throw '托盘隐藏命令未生效。' }
    Send-DisplayPower $window 1
    Start-Sleep -Seconds 1
    # 框架隐藏窗口时也可能一并清除 iconic 标志，必须验证用户关心的“仍隐藏”。
    if ([RecoveryProbe]::IsWindowVisible($window)) { throw '隐藏偏好未保留。' }
    $results += @{Name='hidden-minimized-stays-hidden';Passed=$true;Iconic=[RecoveryProbe]::IsIconic($window)}
    [void][RecoveryProbe]::PostMessage($tray,0x111,[IntPtr]1002,[IntPtr]::Zero)
    $results += Assert-Restored $window $initial 'show-hidden-minimized'
    Send-DisplayPower $window 0
    [void][RecoveryProbe]::SetWindowPos($window,[IntPtr]::Zero,80,90,$initial.Width,22,0x0014)
    # 超过一次10秒检查周期，确认息屏状态不会触发兜底移位。
    Start-Sleep -Seconds 11
    $off = Read-Rect $window
    if ($off.X -ne 80 -or $off.Y -ne 90 -or $off.Height -ne 22) { throw '息屏期间错误地移动了窗口。' }
    Send-DisplayPower $window 1
    $results += Assert-Restored $window $initial 'screen-off-on'
    [void][RecoveryProbe]::SetWindowPos($window,[IntPtr]::Zero,100,110,$initial.Width,22,0x0014)
    [void][RecoveryProbe]::SendMessage($window,0x7e,[IntPtr]32,[IntPtr]::Zero) # WM_DISPLAYCHANGE
    $results += Assert-Restored $window $initial 'display-change'
    # 仅消息模拟用户拖动生命周期，不注入真实鼠标或键盘。
    [void][RecoveryProbe]::SendMessage($window,0x231,[IntPtr]::Zero,[IntPtr]::Zero)
    [void][RecoveryProbe]::SetWindowPos($window,[IntPtr]::Zero,500,300,$initial.Width,$initial.Height,0x0014)
    [void][RecoveryProbe]::SendMessage($window,0x232,[IntPtr]::Zero,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 500
    $dragged = Read-Rect $window
    [void][RecoveryProbe]::SetWindowPos($window,[IntPtr]::Zero,80,90,$initial.Width,22,0x0014)
    [void][RecoveryProbe]::SendMessage($window,0x1a,[IntPtr]::Zero,[IntPtr]::Zero) # WM_SETTINGCHANGE
    $results += Assert-Restored $window $dragged 'saved-user-drag'
    [void][RecoveryProbe]::ShowWindow($window,0)
    Send-DisplayPower $window 0
    Send-DisplayPower $window 1
    Start-Sleep -Seconds 1
    if ([RecoveryProbe]::IsWindowVisible($window)) { throw '恢复时错误地显示了已隐藏窗口。' }
    $results += @{Name='hidden-stays-hidden';Passed=$true}
    [void][RecoveryProbe]::ShowWindow($window,4)
    $report = @{Passed=$true;WindowHandle=$window.ToInt64();Initial=$initial;Final=(Read-Rect $window);Visible=[RecoveryProbe]::IsWindowVisible($window);Cases=$results}
    $report | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $reportPath
    @{Phase='verified';Report=$report} | ConvertTo-Json -Compress -Depth 5
    # 留出离屏 CDP 核验实际 CSS 视口及截图的时间。
    Start-Sleep -Seconds 30
} finally {
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = $previousArgs
    if ($info.Process -ne [IntPtr]::Zero) {
        [void][RecoveryProbe]::TerminateProcess($info.Process,0)
        [void][RecoveryProbe]::CloseHandle($info.Process)
        [void][RecoveryProbe]::CloseHandle($info.Thread)
    }
    [void][RecoveryProbe]::CloseDesktop($desktop)
}

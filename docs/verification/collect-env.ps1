# M0-01 环境信息收集脚本：输出当前 Windows 实机的显示器几何、DPI、键盘布局与输入法。
# 用法：powershell -NoProfile -ExecutionPolicy Bypass -File collect-env.ps1
# 只读取本机配置，不修改任何系统状态。

Add-Type -AssemblyName System.Windows.Forms
Add-Type -MemberDefinition @'
[DllImport("Shcore.dll")]
public static extern int GetDpiForMonitor(IntPtr hmonitor, int dpiType, out uint dpiX, out uint dpiY);
[DllImport("User32.dll")]
public static extern IntPtr MonitorFromPoint(POINT pt, uint flags);
public struct POINT { public int X; public int Y; }
'@ -Name NativeShcore -Namespace RepeatEnv

"=== Monitors (Screen API, per-monitor-DPI aware values) ==="
[System.Windows.Forms.Screen]::AllScreens | ForEach-Object {
    $b = $_.Bounds
    $pt = New-Object RepeatEnv.NativeShcore+POINT
    $pt.X = $b.X + 8
    $pt.Y = $b.Y + 8
    $hmon = [RepeatEnv.NativeShcore]::MonitorFromPoint($pt, 2) # MONITOR_DEFAULTTONEAREST
    $dpiX = 0; $dpiY = 0
    $hr = [RepeatEnv.NativeShcore]::GetDpiForMonitor($hmon, 0, [ref]$dpiX, [ref]$dpiY) # MDT_EFFECTIVE_DPI
    "{0} Primary={1} Bounds=({2},{3}) {4}x{5} DPI={6} (hr=0x{7:X8})" -f $_.DeviceName, $_.Primary, $b.X, $b.Y, $b.Width, $b.Height, $dpiX, $hr
}

"`n=== System DPI (AppliedDPI registry) ==="
(Get-ItemProperty 'HKCU:\Control Panel\Desktop\WindowMetrics' -Name AppliedDPI).AppliedDPI

"`n=== Virtual screen ==="
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SystemInformation]::VirtualScreen | Format-List Left,Top,Width,Height

"`n=== Keyboard layouts / IME ==="
Get-WinUserLanguageList | ForEach-Object {
    "LanguageTag: " + $_.LanguageTag
    "InputMethodTips: " + ($_.InputMethodTips -join ', ')
}

"`n=== Active keyboard layout (current thread) ==="
Add-Type -MemberDefinition '[DllImport("User32.dll")] public static extern IntPtr GetKeyboardLayout(uint idThread);' -Name NativeKbd -Namespace RepeatEnv
$hkl = [RepeatEnv.NativeKbd]::GetKeyboardLayout(0)
"HKL=0x{0:X} (low word = active layout KLID)" -f $hkl.ToInt64()

"`n=== Current user / session ==="
"IsRemoteSession: " + [Environment]::GetEnvironmentVariable("SESSIONNAME")

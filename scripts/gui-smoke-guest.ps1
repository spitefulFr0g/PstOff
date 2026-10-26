# Runs inside the smoke-run Windows Sandbox only (see scripts/gui-smoke.py).
# Launches the GUI on generated OSTs, screenshots each wizard screen, closes it
# and writes the result marker. Every click goes to this guest's own desktop.
$ErrorActionPreference = 'Stop'
$inputDir = 'C:\PstOffSmokeInput'
$outputDir = 'C:\PstOffSmokeOutput'
$work = 'C:\PstOffSmoke'
$result = 1

function Log([string]$message) {
    Add-Content -Path "$outputDir\smoke.log" -Value "$(Get-Date -Format o) $message"
}

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Smoke {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr window, int x, int y, int width, int height, bool repaint);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out RECT rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref POINT point);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    public static void Wheel(int delta) { mouse_event(0x800, 0, 0, unchecked((uint)delta), UIntPtr.Zero); }
}
'@
[Smoke]::SetProcessDPIAware() | Out-Null

function Client($window) {
    $rect = New-Object Smoke+RECT
    [Smoke]::GetClientRect($window, [ref]$rect) | Out-Null
    $origin = New-Object Smoke+POINT
    [Smoke]::ClientToScreen($window, [ref]$origin) | Out-Null
    $dpi = [Smoke]::GetDpiForWindow($window)
    if ($dpi -eq 0) { $dpi = 96 }
    @{ X = $origin.X; Y = $origin.Y; W = $rect.Right; H = $rect.Bottom; Scale = $dpi / 96.0 }
}

function Shot($window, [string]$name) {
    $c = Client $window
    $bitmap = New-Object System.Drawing.Bitmap $c.W, $c.H
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($c.X, $c.Y, 0, 0, $bitmap.Size)
    $bitmap.Save("$outputDir\$name.png", [System.Drawing.Imaging.ImageFormat]::Png)
    $graphics.Dispose()
    $bitmap.Dispose()
    Log "screenshot $name.png ($($c.W)x$($c.H), scale $($c.Scale))"
}

# Coordinates are logical pixels inside the client area.
function Click($window, [double]$x, [double]$y) {
    $c = Client $window
    [Smoke]::SetForegroundWindow($window) | Out-Null
    [Smoke]::SetCursorPos([int]($c.X + $x * $c.Scale), [int]($c.Y + $y * $c.Scale)) | Out-Null
    Start-Sleep -Milliseconds 150
    [Smoke]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    [Smoke]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 900
}

function Scroll($window, [int]$delta) {
    $c = Client $window
    [Smoke]::SetForegroundWindow($window) | Out-Null
    [Smoke]::SetCursorPos([int]($c.X + 700 * $c.Scale), [int]($c.Y + 400 * $c.Scale)) | Out-Null
    [Smoke]::Wheel($delta)
    Start-Sleep -Milliseconds 900
}

# The footer's right-hand button (Next, Review and convert) ends 44 px from the
# right edge, centred about 31 px above the bottom.
function Next($window) {
    $c = Client $window
    Click $window ($c.W / $c.Scale - 44 - 30) ($c.H / $c.Scale - 31)
}

try {
    New-Item -ItemType Directory -Force -Path $work, "$work\fixtures", "$work\out" | Out-Null
    Copy-Item "$inputDir\pstoff-gui.exe" $work
    Copy-Item "$inputDir\fixtures\*.ost" "$work\fixtures"
    $fixtures = @(Get-ChildItem "$work\fixtures\*.ost" | Sort-Object Name | ForEach-Object { '"' + $_.FullName + '"' })
    Log "fixtures: $($fixtures -join ', ')"
    $arguments = @('--out', "`"$work\out`"") + $fixtures
    $process = Start-Process -FilePath "$work\pstoff-gui.exe" -ArgumentList $arguments -PassThru
    for ($i = 0; $i -lt 120 -and $process.MainWindowHandle -eq [IntPtr]::Zero; $i++) {
        if ($process.HasExited) { throw "pstoff-gui.exe exited early with $($process.ExitCode)" }
        Start-Sleep -Milliseconds 500
        $process.Refresh()
    }
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'The PstOff window never appeared.' }
    $window = $process.MainWindowHandle
    Log "window title: $($process.MainWindowTitle)"
    # Fit inside the guest working area: its taskbar must not cover the footer.
    $area = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
    [Smoke]::MoveWindow($window, $area.Left, $area.Top, $area.Width, $area.Height, $true) | Out-Null
    [Smoke]::SetForegroundWindow($window) | Out-Null
    Start-Sleep -Seconds 3
    Shot $window '1-add'
    Scroll $window -1200
    Shot $window '1-add-statuses'
    Scroll $window 2400

    Next $window
    Shot $window '2-folders'
    Next $window
    Shot $window '3-output'
    Scroll $window -1200
    Shot $window '3-output-settings'
    Scroll $window 2400
    Next $window
    Shot $window '4-convert'

    # Start conversion uses the pinned footer, in the same position as Next.
    Next $window
    Shot $window '5-converting'
    for ($i = 0; $i -lt 120; $i++) {
        if (@(Get-ChildItem "$work\out\*.txt" -ErrorAction SilentlyContinue).Count -ge 2) { break }
        Start-Sleep -Milliseconds 500
    }
    if (@(Get-ChildItem "$work\out\*.txt" -ErrorAction SilentlyContinue).Count -lt 2) {
        throw 'Both generated mailboxes did not produce reports.'
    }
    Start-Sleep -Seconds 2
    Shot $window '6-report'
    Scroll $window -1200
    Shot $window '7-report-details'
    Get-ChildItem "$work\out" -Force | ForEach-Object { Log "output: $($_.Name) $($_.Length)" }

    $process.CloseMainWindow() | Out-Null
    if (-not $process.WaitForExit(15000)) {
        $process.Kill()
        throw 'The window did not close.'
    }
    Log "exit code: $($process.ExitCode)"
    if ($process.ExitCode -eq 0) { $result = 0 }
} catch {
    Log "failed: $_"
} finally {
    Set-Content -Path "$outputDir\finished.txt" -Value $result -Encoding Ascii
    shutdown.exe /s /t 5
}

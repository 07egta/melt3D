# Windows-hosted GUI smoke probe. The app's MELTCAD_TEST_DIR hook requests a
# screenshot through egui; the test fails if the app cannot create any image.
param(
    [string]$Exe = "target/debug/meltcad.exe",
    [string]$Output = "ui-snapshots"
)
$ErrorActionPreference = "Stop"
$exePath = (Resolve-Path $Exe).Path
$out = Join-Path (Get-Location) $Output
New-Item -ItemType Directory -Path $out -Force | Out-Null
$env:MELTCAD_TEST_DIR = $out

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class YuraWin {
    [DllImport("user32.dll", SetLastError=true)]
    public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
}
"@
$process = Start-Process -FilePath $exePath -PassThru
try {
    $until = (Get-Date).AddSeconds(40)
    do {
        Start-Sleep -Milliseconds 400
        $process.Refresh()
    } while ($process.MainWindowHandle -eq [IntPtr]::Zero -and !$process.HasExited -and (Get-Date) -lt $until)
    if ($process.HasExited) { throw "YuraCAD exited before its window opened" }
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw "No visible Windows UI handle (possibly a headless runner)" }
    Write-Host "Window handle: $($process.MainWindowHandle)"
    $cases = @(
        @(1280, 720, "ui-simple", "simple-desktop"),
        @(1280, 720, "ui-edit", "edit-desktop"),
        @(1280, 720, "ui-detailed", "detailed-desktop"),
        @(960, 640, "ui-simple", "simple-medium"),
        @(640, 480, "ui-simple", "simple-small"),
        @(640, 480, "ui-edit", "edit-small")
    )
    foreach ($case in $cases) {
        $w = [int]$case[0]
        $h = [int]$case[1]
        $flag = [string]$case[2]
        $label = [string]$case[3]
        if (-not [YuraWin]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 20, 20, $w, $h, 0x0004)) {
            throw "Failed to resize window to $w x $h"
        }
        New-Item -ItemType File -Path (Join-Path $out $flag) -Force | Out-Null
        Start-Sleep -Seconds 1
        $shot = Join-Path $out "shot.ppm"
        Remove-Item $shot -ErrorAction SilentlyContinue
        New-Item -ItemType File -Path (Join-Path $out "snap") -Force | Out-Null
        $limit = (Get-Date).AddSeconds(30)
        while (!(Test-Path $shot) -and (Get-Date) -lt $limit) {
            Start-Sleep -Milliseconds 300
            $process.Refresh()
            if ($process.HasExited) { throw "YuraCAD exited while rendering $label" }
        }
        if (!(Test-Path $shot)) { throw "No screenshot captured for $label" }
        $destination = Join-Path $out "$label.ppm"
        Move-Item $shot $destination -Force
        Write-Host "$label : $((Get-Item $destination).Length) bytes"
    }
} finally {
    New-Item -ItemType File -Path (Join-Path $out "quit") -Force | Out-Null
    Start-Sleep -Seconds 1
    $process.Refresh()
    if (!$process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
}

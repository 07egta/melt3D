# Measure Windows GUI launch, library-first-frame and 100+ search-result browsing.
param(
    [Parameter(Mandatory=$true)][string]$Before,
    [Parameter(Mandatory=$true)][string]$After,
    [int]$Runs = 3,
    [int]$ScrollSteps = 115,
    [int]$SearchY = 115,
    [int]$ScrollY = 455,
    [string]$Output = "yuracad-startup-comparison.csv"
)
$ErrorActionPreference = "Stop"
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw "Windows GUI required." }
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class YuraInput {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, int wheel, UIntPtr extra);
}
'@
$Before = (Resolve-Path $Before).Path
$After = (Resolve-Path $After).Path
$results = New-Object System.Collections.Generic.List[object]

function Await-Path($path, $proc, $timeoutSeconds, [bool]$expected) {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while ($clock.Elapsed.TotalSeconds -lt $timeoutSeconds) {
        if ((Test-Path -LiteralPath $path) -eq $expected) { return }
        $proc.Refresh()
        if ($proc.HasExited) { throw "Application exited before $path reached $expected" }
        Start-Sleep -Milliseconds 25
    }
    throw "Timeout for $path"
}
function Run-One($name, $exe, $iteration) {
    $dir = Join-Path ([IO.Path]::GetTempPath()) ("yuracad-speed-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $dir | Out-Null
    New-Item -ItemType File -Path (Join-Path $dir "snap") | Out-Null
    $oldDir = $env:MELTCAD_TEST_DIR
    $oldTiming = $env:MELTCAD_TIMING
    $env:MELTCAD_TEST_DIR = $dir
    $env:MELTCAD_TIMING = "1"
    $proc = $null
    try {
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $proc = Start-Process -FilePath $exe -PassThru
        Await-Path (Join-Path $dir "shot.ppm") $proc 45 $true
        $startMs = [math]::Round($timer.Elapsed.TotalMilliseconds, 1)
        $proc.Refresh()
        $startPeak = [math]::Round($proc.PeakWorkingSet64 / 1MB, 1)
        Remove-Item (Join-Path $dir "shot.ppm")
        $timer.Restart()
        $marker = Join-Path $dir "lib"
        New-Item -ItemType File -Path $marker | Out-Null
        Await-Path $marker $proc 20 $false
        New-Item -ItemType File -Path (Join-Path $dir "snap") | Out-Null
        Await-Path (Join-Path $dir "shot.ppm") $proc 45 $true
        $libraryMs = [math]::Round($timer.Elapsed.TotalMilliseconds, 1)
        $proc.Refresh()
        $libraryPeak = [math]::Round($proc.PeakWorkingSet64 / 1MB, 1)
        Remove-Item (Join-Path $dir "shot.ppm")

        $proc.Refresh()
        $handle = $proc.MainWindowHandle
        if ($handle -eq [IntPtr]::Zero) { throw "Window handle unavailable" }
        [YuraInput]::SetForegroundWindow($handle) | Out-Null
        $rect = New-Object YuraInput+RECT
        if (-not [YuraInput]::GetWindowRect($handle, [ref]$rect)) { throw "GetWindowRect failed" }
        [YuraInput]::SetCursorPos(($rect.Left + 135), ($rect.Top + $SearchY)) | Out-Null
        [YuraInput]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        [YuraInput]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        [System.Windows.Forms.SendKeys]::SendWait("e")
        Start-Sleep -Milliseconds 250
        $timer.Restart()
        [YuraInput]::SetCursorPos(($rect.Left + 150), ($rect.Top + $ScrollY)) | Out-Null
        for ($j = 0; $j -lt $ScrollSteps; $j++) {
            [YuraInput]::mouse_event(0x0800, 0, 0, -120, [UIntPtr]::Zero)
            Start-Sleep -Milliseconds 30
        }
        New-Item -ItemType File -Path (Join-Path $dir "snap") | Out-Null
        Await-Path (Join-Path $dir "shot.ppm") $proc 45 $true
        $browseMs = [math]::Round($timer.Elapsed.TotalMilliseconds, 1)
        $proc.Refresh()
        [pscustomobject]@{
            Version = $name
            Iteration = $iteration
            StartToFirstScreenshotMs = $startMs
            StartupPeakWorkingSetMiB = $startPeak
            LibraryFirstFrameMs = $libraryMs
            LibraryPeakWorkingSetMiB = $libraryPeak
            Browse100PlusMatchesMs = $browseMs
            PeakWorkingSetMiB = [math]::Round($proc.PeakWorkingSet64 / 1MB, 1)
        }
    } finally {
        $env:MELTCAD_TEST_DIR = $oldDir
        $env:MELTCAD_TIMING = $oldTiming
        if ($proc -and -not $proc.HasExited) {
            New-Item -ItemType File -Path (Join-Path $dir "quit") -Force | Out-Null
            if (-not $proc.WaitForExit(3000)) { $proc.Kill() }
        }
        Remove-Item -LiteralPath $dir -Force -Recurse -ErrorAction SilentlyContinue
    }
}
for ($i = 1; $i -le $Runs; $i++) {
    $order = if ($i % 2) { @(@("before", $Before), @("after", $After)) } else { @(@("after", $After), @("before", $Before)) }
    foreach ($pair in $order) {
        $sample = Run-One $pair[0] $pair[1] $i
        $results.Add($sample)
        $sample | Format-Table -AutoSize
    }
}
$results | Export-Csv -LiteralPath $Output -Encoding UTF8 -NoTypeInformation
Write-Host "Saved: $Output"
Write-Host "NOTE: A fresh process is not a cold OS disk cache. Reboot between cold samples."
Write-Host "Check the search field actually contains 'e'; adjust SearchY/ScrollY for your DPI and window."

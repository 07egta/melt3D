# Compare BEFORE and AFTER Windows Release builds on the same Windows machine.
# Run with -RunGpuBench to compare existing built-in 300-shape GPU speed tests.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$BeforeReleaseExe,
    [Parameter(Mandatory=$true)][string]$AfterReleaseExe,
    [string]$OutputCsv = ".\yuracad-footprint.csv",
    [ValidateRange(2,120)][int]$WarmupSeconds = 12,
    [ValidateRange(2,30)][int]$SampleSeconds = 5,
    [switch]$RunGpuBench
)
$ErrorActionPreference = 'Stop'
$before = (Resolve-Path -LiteralPath $BeforeReleaseExe).ProviderPath
$after = (Resolve-Path -LiteralPath $AfterReleaseExe).ProviderPath
if ($before -eq $after) { throw 'Choose two different Release EXE files' }
if ([IO.Path]::GetExtension($before) -ne '.exe' -or [IO.Path]::GetExtension($after) -ne '.exe') {
    throw 'Both input files must be Windows release executables'
}
$dir = Join-Path $env:TEMP ('yuracad-footprint-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$fixture = Join-Path $dir 'exactly-300-shapes.meltcad'
$rows = @()
$oldDir = [Environment]::GetEnvironmentVariable('MELTCAD_TEST_DIR','Process')
$oldQuiet = [Environment]::GetEnvironmentVariable('MELTCAD_BENCH_QUIET','Process')

function Sample-Ram($proc, $seconds) {
    $working = @(); $private = @(); $peak = @()
    for ($k=0; $k -lt $seconds*2; $k++) {
        $proc.Refresh()
        if ($proc.HasExited) { throw 'YuraCAD exited during memory sampling' }
        $working += [double]$proc.WorkingSet64
        $private += [double]$proc.PrivateMemorySize64
        $peak += [double]$proc.PeakWorkingSet64
        Start-Sleep -Milliseconds 500
    }
    return [pscustomobject]@{
        WS = [math]::Round(($working | Measure-Object -Average).Average / 1MB, 2)
        Private = [math]::Round(($private | Measure-Object -Average).Average / 1MB, 2)
        Peak = [math]::Round(($peak | Measure-Object -Maximum).Maximum / 1MB, 2)
    }
}

function Test-Window($tag, $exe, $model) {
    $ctrl = Join-Path $dir ($tag + '-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $ctrl | Out-Null
    [Environment]::SetEnvironmentVariable('MELTCAD_TEST_DIR', $ctrl, 'Process')
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $proc = $null
    try {
        if ($model) {
            $proc = Start-Process -FilePath $exe -ArgumentList ('"' + $model + '"') -PassThru
        } else {
            $proc = Start-Process -FilePath $exe -PassThru
        }
        $startupMs = $null
        while ($sw.Elapsed.TotalSeconds -lt 90) {
            Start-Sleep -Milliseconds 100
            $proc.Refresh()
            if ($proc.HasExited) { throw "Process $tag exited before opening a window" }
            if ($proc.MainWindowHandle -ne 0) {
                $startupMs = [math]::Round($sw.Elapsed.TotalMilliseconds,0)
                break
            }
        }
        if ($null -eq $startupMs) { throw "Process $tag failed to open a window" }
        Start-Sleep -Seconds $WarmupSeconds
        $sample = Sample-Ram $proc $SampleSeconds
        $phase = if ($model) { '300-loaded' } else { 'idle' }
        $script:rows += [pscustomobject]@{
            Build=$tag; Phase=$phase; ExeBytes=(Get-Item -LiteralPath $exe).Length
            StartupToWindowMs=$startupMs; WorkingSetMiB=$sample.WS
            PrivateMiB=$sample.Private; PeakWSMiB=$sample.Peak
        }
        if ($model) {
            $ack = Join-Path $ctrl 'reset_scene.done'
            New-Item -ItemType File -Force -Path (Join-Path $ctrl 'reset_scene') | Out-Null
            $until = (Get-Date).AddSeconds(20)
            while (!(Test-Path -LiteralPath $ack) -and (Get-Date) -lt $until) {
                if ($proc.HasExited) { throw 'Process died during model close' }
                Start-Sleep -Milliseconds 200
            }
            if (!(Test-Path -LiteralPath $ack)) { throw 'Model-close test hook did not respond' }
            if ((Get-Content -LiteralPath $ack -Raw).Trim() -ne '1') {
                throw 'Model was not cleared to the initial one-shape scene'
            }
            Start-Sleep -Seconds $WarmupSeconds
            $sample = Sample-Ram $proc $SampleSeconds
            $script:rows += [pscustomobject]@{
                Build=$tag; Phase='300-closed'; ExeBytes=(Get-Item -LiteralPath $exe).Length
                StartupToWindowMs=$null; WorkingSetMiB=$sample.WS
                PrivateMiB=$sample.Private; PeakWSMiB=$sample.Peak
            }
        }
    } finally {
        [Environment]::SetEnvironmentVariable('MELTCAD_TEST_DIR',$oldDir,'Process')
        if ($null -ne $proc) {
            try {
                if (!$proc.HasExited) {
                    New-Item -ItemType File -Force -Path (Join-Path $ctrl 'quit') | Out-Null
                    if (!$proc.WaitForExit(5000)) { Stop-Process -Id $proc.Id -Force }
                }
            } finally { $proc.Dispose() }
        }
    }
}

function Test-Gpu($tag, $exe) {
    $output = Join-Path $dir ($tag + '-gpu.txt')
    [Environment]::SetEnvironmentVariable('MELTCAD_TEST_DIR',$null,'Process')
    [Environment]::SetEnvironmentVariable('MELTCAD_BENCH_QUIET','1','Process')
    try {
        $args = @('--bench', ('"' + $output + '"'), '--quick', '--frames', '3')
        $p = Start-Process -FilePath $exe -ArgumentList $args -PassThru -Wait
        if ($p.ExitCode -ne 0 -or !(Test-Path -LiteralPath $output)) { throw 'GPU benchmark failed' }
        $matches300 = @(Get-Content -LiteralPath $output | Where-Object {
            $_ -match '^\s*shapes300\s+\d+\s+1280x720\s+(full|fast)\s+([0-9.]+)'
        })
        if ($matches300.Count -ne 2) { throw 'Expected full and fast 300-shape rows in GPU report' }
        Write-Host "$tag GPU speed results:"
        $matches300 | ForEach-Object { Write-Host $_ }
        Write-Host "Full GPU benchmark: $output"
    } finally {
        [Environment]::SetEnvironmentVariable('MELTCAD_TEST_DIR',$oldDir,'Process')
        [Environment]::SetEnvironmentVariable('MELTCAD_BENCH_QUIET',$oldQuiet,'Process')
    }
}

try {
    # Generate with the candidate; use the same fixture bytes for both builds.
    $p = Start-Process -FilePath $after -ArgumentList @('--memory-fixture', ('"' + $fixture + '"')) -PassThru -Wait
    if ($p.ExitCode -ne 0 -or !(Test-Path -LiteralPath $fixture)) { throw 'Could not generate the 300-shape fixture' }
    Write-Host ('Fixture SHA256: ' + (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash)
    foreach ($entry in @(@('before-release',$before),@('after-release',$after))) {
        Test-Window $entry[0] $entry[1] $null
        Test-Window $entry[0] $entry[1] $fixture
        if ($RunGpuBench) { Test-Gpu $entry[0] $entry[1] }
    }
    $csv = [IO.Path]::GetFullPath($OutputCsv)
    $rows | Export-Csv -NoTypeInformation -Encoding UTF8 -LiteralPath $csv
    $rows | Format-Table Build,Phase,ExeBytes,StartupToWindowMs,WorkingSetMiB,PrivateMiB,PeakWSMiB -AutoSize
    Write-Host "Measurement CSV: $csv"
    Write-Host 'Startup ends at the first window, not the first finished GPU frame.'
    Write-Host 'WorkingSet/Private measure process RAM, not dedicated GPU VRAM.'
} finally {
    [Environment]::SetEnvironmentVariable('MELTCAD_TEST_DIR',$oldDir,'Process')
    [Environment]::SetEnvironmentVariable('MELTCAD_BENCH_QUIET',$oldQuiet,'Process')
}

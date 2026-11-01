#Requires -Version 7.0
[CmdletBinding()]
param(
    [ValidateSet('codex','claude')][string]$Agent = 'codex',
    [Parameter(Mandatory)][string]$HarnessPath,
    [Parameter(Mandatory)][string]$AheraldPath,
    [Parameter(Mandatory)][string]$BridgePath
)
# Opt-in real-account source -> production adapter/IPC -> native resident check.
$ErrorActionPreference = 'Stop'
$AheraldPath = (Resolve-Path -LiteralPath $AheraldPath).Path
$BridgePath = (Resolve-Path -LiteralPath $BridgePath).Path
$root = Join-Path $env:TEMP ('aherald-context-live-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$scope = 'ah101-' + [guid]::NewGuid().ToString('N')
$config = Join-Path $root 'config.toml'
@'
[sounds]
enabled=false
[codex.usage]
enabled=false
[claude.usage]
enabled=false
[claude.warnings]
context_tokens=1
idle_minutes=0
[codex.warnings]
context_tokens=1
idle_minutes=0
[events.context_warning]
duration_ms=0
'@ | Set-Content -LiteralPath $config
function New-Info([string]$File, [string[]]$Arguments) {
    $info = [Diagnostics.ProcessStartInfo]::new($File)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.Environment['AH101_SCOPE'] = $scope
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    return $info
}
function Invoke-Bridge([string[]]$Arguments) {
    $p = [Diagnostics.Process]::Start((New-Info $BridgePath $Arguments))
    try {
        $p.StandardInput.Close()
        $output = $p.StandardOutput.ReadToEndAsync()
        $errorOutput = $p.StandardError.ReadToEndAsync()
        if (-not $p.WaitForExit(5000)) { throw 'Bridge command timed out.' }
        if ($p.ExitCode -ne 0) { throw 'Bridge command failed.' }
        return $output.Result
    } finally {
        if (-not $p.HasExited) { $p.Kill($true); $null = $p.WaitForExit(5000) }
        $p.Dispose()
    }
}
$resident = $null
$probe = $null
try {
    $resident = [Diagnostics.Process]::Start((New-Info $AheraldPath @('--resident-child',$scope,'--config',$config)))
    $resident.StandardInput.Close()
    $residentOut = $resident.StandardOutput.ReadToEndAsync()
    $residentErr = $resident.StandardError.ReadToEndAsync()
    $ready = [Diagnostics.Stopwatch]::StartNew()
    do {
        $status = Invoke-Bridge @('--status') | ConvertFrom-Json
        if ($status.result -eq 'status') { break }
        if ($resident.HasExited -or $ready.Elapsed.TotalSeconds -gt 10) { throw 'Resident not ready.' }
        Start-Sleep -Milliseconds 100
    } while ($true)
    if ($status.pid -ne $resident.Id -or $status.queued -ne 0) { throw 'Resident ownership or empty-queue check failed.' }
    if ($Agent -eq 'codex') {
        $info = New-Info 'pwsh.exe' @('-NoProfile','-File',(Join-Path $PSScriptRoot 'codex_context_source.ps1'),'-CodexPath',$HarnessPath)
        $info.Environment['AH100_BRIDGE'] = $BridgePath
    } else {
        $settings = Join-Path $root 'settings.json'
        $command = '"' + $BridgePath + '" --statusline'
        @{statusLine=@{type='command';command=$command};enabledPlugins=@{}} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $settings
        $info = New-Info $HarnessPath @('--setting-sources','','--settings',$settings,'--strict-mcp-config','--tools','','--no-session-persistence','Reply with exactly OK. Do not use tools or inspect files.')
        $info.RedirectStandardInput = $false
        $info.RedirectStandardOutput = $false
        $info.RedirectStandardError = $false
        $info.Environment['AH100_CONTEXT_ROOT'] = $root
        $info.WorkingDirectory = $root
        Write-Output 'Claude interactive acceptance: accept workspace trust if requested; wait for the context warning, then /exit.'
    }
    $probe = [Diagnostics.Process]::Start($info)
    if ($Agent -eq 'codex') {
        $probe.StandardInput.Close()
        $probeOut = $probe.StandardOutput.ReadToEndAsync()
        $probeErr = $probe.StandardError.ReadToEndAsync()
    }
    if (-not $probe.WaitForExit(105000)) { throw 'Source probe deadline exceeded.' }
    if ($Agent -eq 'codex') {
        $probeOut.Result | Set-Content -LiteralPath (Join-Path $root 'source-result.txt')
    }
    if ($probe.ExitCode -ne 0) { throw 'Source probe failed.' }
    $status = Invoke-Bridge @('--status') | ConvertFrom-Json
    if ($status.queued -ne 1) { throw 'Expected exactly one native context warning.' }
    $status | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'warning.json')
    Write-Output "PASS: real $Agent context produced one warning in the isolated native resident. Appearance/focus are separate manual checks."
} finally {
    if ($null -ne $probe) {
        if (-not $probe.HasExited) { $probe.Kill($true); $null = $probe.WaitForExit(5000) }
        $probe.Dispose()
    }
    if ($null -ne $resident) {
        if (-not $resident.HasExited) {
            $null = Invoke-Bridge @('--quit')
            if (-not $resident.WaitForExit(5000)) { $resident.Kill($true); $null = $resident.WaitForExit(5000) }
        }
        $resident.Dispose()
    }
    Write-Output "Evidence: $root"
}

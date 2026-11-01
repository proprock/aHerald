#Requires -Version 7.0
[CmdletBinding()]
param(
    [string]$CodexPath,
    [switch]$Record,
    [string]$EvidenceDirectory
)
# Opt-in compatibility research, not an installed aHerald hook. Never emits control JSON.
$ErrorActionPreference = 'Stop'
if ($Record) {
    $wire = [Console]::In.ReadToEnd() | ConvertFrom-Json
    if ($wire.hook_event_name -notin @('PreModelRequest','PostModelRequest')) { exit 1 }
    $entryRecord = [ordered]@{
        event = $wire.hook_event_name
        session = $wire.session_id
        turn = $wire.turn_id
        subagent = [bool]$wire.agent_id
    }
    if ($wire.hook_event_name -eq 'PostModelRequest') {
        $entryRecord.status = $wire.status
        $entryRecord.usage = $null
        if ($null -ne $wire.token_usage) {
            $entryRecord.usage = [ordered]@{}
            foreach ($field in @('input_tokens','cached_input_tokens','cache_write_input_tokens','output_tokens','reasoning_output_tokens','total_tokens')) {
                $value = $wire.token_usage.$field
                if ($null -ne $value -and ($value -is [long] -or $value -is [int])) {
                    $entryRecord.usage[$field] = $value
                }
            }
        }
        if ($env:AH100_BRIDGE) {
            $bridgeInfo = [Diagnostics.ProcessStartInfo]::new($env:AH100_BRIDGE)
            $bridgeInfo.UseShellExecute = $false
            $bridgeInfo.CreateNoWindow = $true
            $bridgeInfo.RedirectStandardInput = $true
            $bridgeInfo.RedirectStandardOutput = $true
            $bridgeInfo.RedirectStandardError = $true
            foreach ($argument in @('--hook','--resident')) { $bridgeInfo.ArgumentList.Add($argument) }
            $bridge = [Diagnostics.Process]::Start($bridgeInfo)
            try {
                $bridge.StandardInput.Write(($wire | ConvertTo-Json -Depth 20 -Compress))
                $bridge.StandardInput.Close()
                $bridgeOut = $bridge.StandardOutput.ReadToEndAsync()
                $bridgeErr = $bridge.StandardError.ReadToEndAsync()
                if (-not $bridge.WaitForExit(3000)) { throw 'Context bridge exceeded three seconds.' }
                if ($bridge.ExitCode -ne 0 -or $bridgeOut.Result -ne '') { throw 'Context bridge contract failed.' }
            } finally {
                if (-not $bridge.HasExited) { $bridge.Kill($true); $null = $bridge.WaitForExit(5000) }
                $bridge.Dispose()
            }
        }
    }
    # Never retain cwd, transcript_path, model, errors, prompt or raw hook JSON.
    $target = Join-Path $EvidenceDirectory ($wire.hook_event_name + '-' + [guid]::NewGuid().ToString('N') + '.json')
    $entryRecord | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $target
    exit 0
}
if (-not $IsWindows) { throw 'Windows is required.' }
if (-not $CodexPath) { throw 'Supply the native CodexPath.' }
$CodexPath = (Resolve-Path -LiteralPath $CodexPath).Path
$root = Join-Path $env:TEMP ('aherald-codex-context-' + [guid]::NewGuid().ToString('N'))
$workspace = Join-Path $root 'workspace'
New-Item -ItemType Directory -Path $workspace | Out-Null
$version = & $CodexPath --version
if ($LASTEXITCODE -ne 0) { throw 'Could not read Codex version.' }
$version | Set-Content -LiteralPath (Join-Path $root 'version.txt')
$recorder = Join-Path $root 'record.ps1'
Copy-Item -LiteralPath $PSCommandPath -Destination $recorder
$command = 'pwsh.exe -NoProfile -NonInteractive -File "' + $recorder + '" -Record -EvidenceDirectory "' + $root + '"'
$quoted = $command | ConvertTo-Json -Compress
$cliArgs = @('--no-daemon','exec','--ignore-user-config','--ignore-rules','--ephemeral','--skip-git-repo-check',
    '--dangerously-bypass-hook-trust','--disable','plugins','--disable','apps','--disable','remote_plugin',
    '--disable','plugin_discovery_prefetch','-C',$workspace,'-s','read-only','-c','analytics.enabled=false')
foreach ($name in @('PreModelRequest','PostModelRequest')) {
    $cliArgs += @('-c', ('hooks.' + $name + '=[{hooks=[{type="command",command=' + $quoted + ',commandWindows=' + $quoted + ',timeout=5}]}]'))
}
$cliArgs += @('Reply with exactly OK. Do not use tools or inspect files.')
$info = [Diagnostics.ProcessStartInfo]::new($CodexPath)
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardInput = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$info.WorkingDirectory = $workspace
foreach ($argument in $cliArgs) { $info.ArgumentList.Add($argument) }
$process = $null
try {
    $process = [Diagnostics.Process]::Start($info)
    $process.StandardInput.Close()
    # Drain both pipes; retain no harness payload or account diagnostics.
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(90000)) { throw 'Codex exceeded the 90-second deadline.' }
    if ($process.ExitCode -ne 0) { throw "Codex failed with exit $($process.ExitCode); raw diagnostics were not retained." }
    if ($stdout.Result.Trim() -ne 'OK') { throw 'Unexpected model response; raw output was not retained.' }
    $pre = @(Get-ChildItem -LiteralPath $root -Filter 'PreModelRequest-*.json')
    $post = @(Get-ChildItem -LiteralPath $root -Filter 'PostModelRequest-*.json' | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json })
    if ($pre.Count -eq 0) { throw 'No real PreModelRequest dispatch.' }
    $valid = @($post | Where-Object { $_.status -eq 'completed' -and -not $_.subagent -and $_.session -and $_.turn -and $null -ne $_.usage -and $_.usage.input_tokens -gt 0 })
    if ($valid.Count -eq 0) { throw 'No completed main-session PostModelRequest with input usage.' }
    Write-Output 'PASS: real model response and model-request hooks with per-request input usage. Stability and notification delivery are separate checks.'
    $valid | ConvertTo-Json -Depth 5
} finally {
    if ($null -ne $process) {
        if (-not $process.HasExited) { $process.Kill($true) }
        if (-not $process.WaitForExit(5000)) { throw 'Owned Codex process cleanup timed out.' }
        $process.Dispose()
    }
    Write-Output "Evidence: $root"
}

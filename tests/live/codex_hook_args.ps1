#Requires -Version 7.0
[CmdletBinding()]
param([Parameter(Mandatory)][string]$CodexPath)
# Requires PowerShell 7 and Windows. Run explicitly with a vetted Codex executable.
# This opt-in compatibility probe is independent of cargo test and real user hooks.
if (-not $IsWindows) { throw 'This probe requires Windows.' }
$CodexPath = (Resolve-Path -LiteralPath $CodexPath -ErrorAction Stop).Path
if ([IO.Path]::GetExtension($CodexPath) -ne '.exe') { throw 'Supply the native codex.exe, not a shell shim.' }
& $CodexPath --version
if ($LASTEXITCODE -ne 0) { throw 'Could not read Codex version.' }
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP ('aherald-codex-hook-args-' + [guid]::NewGuid().ToString('N'))
$probeHome = Join-Path $root 'home'
$workspace = Join-Path $root 'workspace'
New-Item -ItemType Directory -Path $probeHome,$workspace | Out-Null
$recorder = Join-Path $root 'record.ps1'
@('$label = $args[0]', '$received = @($args | Select-Object -Skip 1)', '$target = Join-Path $env:AHERALD_ARGS_RECORD ($label + ".json")', '@{ label = $label; arguments = $received } | ConvertTo-Json -Compress | Set-Content -LiteralPath ($target + ".tmp") -Encoding utf8', 'Move-Item -LiteralPath ($target + ".tmp") -Destination $target') | Set-Content -LiteralPath $recorder
$command = 'powershell.exe -NoProfile -NonInteractive -File "' + $recorder + '" '
@{hooks=@{SessionStart=@(@{hooks=@(@{type='command';command=($command + 'control inline-sentinel');timeout=5},@{type='command';command=($command + 'candidate');args=@('separate-sentinel','two words');timeout=5})})}} | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $probeHome 'hooks.json')
$info = [System.Diagnostics.ProcessStartInfo]::new($CodexPath)
$info.UseShellExecute=$false
$info.RedirectStandardOutput=$true
$info.RedirectStandardError=$true
$info.RedirectStandardInput=$true
$info.Environment['CODEX_HOME']=$probeHome
$info.Environment['AHERALD_ARGS_RECORD']=$root
# Trust bypass is limited to our two generated recorders in a fresh CODEX_HOME.
# Disable startup plugin synchronization; the model endpoint is a closed loopback port.
$cliArgs=@('--no-daemon','exec','--ignore-user-config','--ignore-rules','--ephemeral','--skip-git-repo-check','--dangerously-bypass-hook-trust','--disable','plugins','--disable','apps','--disable','remote_plugin','--disable','plugin_discovery_prefetch','-C',$workspace,'-s','read-only','-c','analytics.enabled=false','-c','model_provider="probe"','-c','model_providers.probe.name="probe"','-c','model_providers.probe.base_url="http://127.0.0.1:9/v1"','-c','model_providers.probe.wire_api="responses"','-c','model_providers.probe.requires_openai_auth=false','-c','model_providers.probe.request_max_retries=0','-c','model_providers.probe.stream_max_retries=0','-m','probe','Do not execute tools.')
foreach($item in $cliArgs){$info.ArgumentList.Add($item)}
$process=$null
try {
    $process=[System.Diagnostics.Process]::Start($info)
    $process.StandardInput.Close()
    $stdout=$process.StandardOutput.ReadToEndAsync()
    $stderr=$process.StandardError.ReadToEndAsync()
    $deadline=[System.Diagnostics.Stopwatch]::StartNew()
    while($deadline.Elapsed.TotalSeconds -lt 25 -and -not $process.HasExited) {
        if((Test-Path (Join-Path $root 'control.json')) -and (Test-Path (Join-Path $root 'candidate.json'))){break}
        Start-Sleep -Milliseconds 50
    }
} finally {
    if($null -ne $process) {
        if(-not $process.HasExited){$process.Kill($true)}
        if(-not $process.WaitForExit(5000)){throw "Codex cleanup timed out; logs: $root"}
        $stdout.Result | Set-Content -LiteralPath (Join-Path $root 'stdout.log')
        $stderr.Result | Set-Content -LiteralPath (Join-Path $root 'stderr.log')
        $process.Dispose()
    }
}
Write-Output "Evidence: $root"
$control=Get-Content -LiteralPath (Join-Path $root 'control.json') -Raw | ConvertFrom-Json
$candidate=Get-Content -LiteralPath (Join-Path $root 'candidate.json') -Raw | ConvertFrom-Json
if($control.label -ne 'control' -or @($control.arguments).Count -ne 1 -or $control.arguments[0] -ne 'inline-sentinel'){throw 'Control hook did not receive inline argument'}
if($candidate.label -ne 'candidate'){throw 'Candidate hook did not execute'}
if(@($candidate.arguments).Count -ne 0){throw 'Codex now forwards args: review native setup architecture'}
Write-Output 'PASS: inline command argument received; separate args field silently ignored.'
Get-Content -LiteralPath (Join-Path $root 'control.json'),(Join-Path $root 'candidate.json')

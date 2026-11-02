#Requires -Version 7.0
[CmdletBinding()]
param(
    [string]$CodexPath,
    [switch]$Record,
    [string]$EvidenceDirectory
)
# Opt-in research: observe a harmless escalation, never answer an approval.
$ErrorActionPreference = 'Stop'
if ($Record) {
    $wire = [Console]::In.ReadToEnd() | ConvertFrom-Json
    if ($wire.hook_event_name -ne 'PermissionRequest') { exit 1 }
    # Record field presence only. Never retain arguments, paths or raw payloads.
    $knownFields = @('session_id','turn_id','agent_id','agent_type','cwd','model',
        'permission_mode','tool_name','tool_input','transcript_path')
    $fields = @($knownFields | Where-Object { $_ -in $wire.PSObject.Properties.Name })
    $mode = if ($wire.permission_mode -in @('default','acceptEdits','plan','dontAsk','bypassPermissions')) {
        $wire.permission_mode
    } else { 'unknown' }
    @{event='PermissionRequest'; fields=$fields; permission_mode=$mode} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $EvidenceDirectory ('hook-' + [guid]::NewGuid().ToString('N') + '.json'))
    exit 0
}
if (-not $IsWindows -or -not $CodexPath) { throw 'Supply a native Windows CodexPath.' }
$CodexPath = (Resolve-Path -LiteralPath $CodexPath).Path
if ([IO.Path]::GetExtension($CodexPath) -ne '.exe') { throw 'Use native codex.exe, not a shell shim.' }
$root = Join-Path $env:TEMP ('aherald-ah099-' + [guid]::NewGuid().ToString('N'))
$workspace = Join-Path $root 'workspace'
New-Item -ItemType Directory -Path $workspace | Out-Null
& $CodexPath --version | Set-Content -LiteralPath (Join-Path $root 'version.txt')
if ($LASTEXITCODE -ne 0) { throw 'Could not read Codex version.' }
$recorder = Join-Path $root 'record.ps1'
Copy-Item -LiteralPath $PSCommandPath -Destination $recorder
foreach ($reviewer in @('user','auto_review')) {
    $evidence = Join-Path $root $reviewer
    New-Item -ItemType Directory -Path $evidence | Out-Null
    $command = 'pwsh.exe -NoProfile -NonInteractive -File "' + $recorder + '" -Record -EvidenceDirectory "' + $evidence + '"'
    $quoted = $command | ConvertTo-Json -Compress
    $cliArgs = @('--no-daemon','exec','--ignore-user-config','--ignore-rules','--ephemeral',
        '--skip-git-repo-check','--dangerously-bypass-hook-trust',
        '--disable','plugins','--disable','apps','--disable','remote_plugin',
        '--disable','plugin_discovery_prefetch','-C',$workspace,'-s','read-only',
        '-c','analytics.enabled=false','-c','approval_policy="on-request"',
        '-c',('approvals_reviewer="' + $reviewer + '"'),
        '-c',('hooks.PermissionRequest=[{hooks=[{type="command",command=' + $quoted + ',commandWindows=' + $quoted + ',timeout=5}]}]'),
        'Compatibility probe: use the shell tool exactly once to run Write-Output AH099 with sandbox_permissions=require_escalated and a justification explaining that this harmless output tests approval-hook dispatch. Do not inspect files, use other tools, or retry if rejected. Then finish.')
    $info = [Diagnostics.ProcessStartInfo]::new($CodexPath)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.WorkingDirectory = $workspace
    foreach ($argument in $cliArgs) { $info.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::Start($info)
    try {
        $process.StandardInput.Close()
        # Drain output but do not persist model/account diagnostics.
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        $completed = $process.WaitForExit(55000)
        $exitCode = if ($completed) { $process.ExitCode } else { $null }
        @{configured_reviewer=$reviewer; completed=$completed; exit_code=$exitCode;
            hook_count=@(Get-ChildItem -LiteralPath $evidence -Filter 'hook-*.json').Count} |
            ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'summary.json')
    } finally {
        if (-not $process.HasExited) { $process.Kill($true) }
        if (-not $process.WaitForExit(5000)) { throw 'Owned Codex cleanup timed out.' }
        $process.Dispose()
        Write-Output "Evidence: $evidence"
    }
}
Write-Output 'Dispatch evidence only: configured reviewer, exit status and hook entry do not prove an approval outcome or an open human prompt.'

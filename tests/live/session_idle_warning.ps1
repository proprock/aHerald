#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('claude','codex')][string]$Agent,
    [Parameter(Mandatory)][string]$HarnessPath,
    [Parameter(Mandatory)][string]$AheraldPath,
    [Parameter(Mandatory)][string]$BridgePath,
    [switch]$Interactive
)
# Opt-in account test. A separate Stop hook keeps the print-mode session alive;
# the actual aHerald handler must still finish in under three seconds.
$ErrorActionPreference='Stop'
if(-not $IsWindows){throw 'Windows is required.'}
if($Interactive -and $Agent -ne 'codex'){throw 'Interactive mode is currently Codex only.'}
$HarnessPath=(Resolve-Path -LiteralPath $HarnessPath).Path
$AheraldPath=(Resolve-Path -LiteralPath $AheraldPath).Path
$BridgePath=(Resolve-Path -LiteralPath $BridgePath).Path
$scope='ah101-'+[guid]::NewGuid().ToString('N')
$root=Join-Path $env:TEMP ('aherald-idle-live-'+[guid]::NewGuid().ToString('N'))
$workspace=Join-Path $root 'workspace'
New-Item -ItemType Directory -Path $workspace | Out-Null
$scope | Set-Content -LiteralPath (Join-Path $root 'scope.txt')
$config=Join-Path $root 'config.toml'
@'
[general]
logging='debug'
[sounds]
enabled=false
[codex.usage]
enabled=false
[claude.usage]
enabled=false
[claude.warnings]
idle_minutes=1
[codex.warnings]
idle_minutes=1
[events.turn_complete]
enabled=false
[events.idle_warning]
duration_ms=0
'@ | Set-Content -LiteralPath $config
function New-Info([string]$File,[string[]]$Arguments){
    $info=[Diagnostics.ProcessStartInfo]::new($File)
    $info.UseShellExecute=$false;$info.CreateNoWindow=$true
    $info.RedirectStandardInput=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
    $info.WorkingDirectory=$workspace
    foreach($argument in $Arguments){$info.ArgumentList.Add($argument)}
    $info.Environment['AH101_ROOT']=$root
    $info.Environment['AH101_EXE']=$AheraldPath
    $info.Environment['AH101_AGENT']=$Agent
    $info.Environment['AH101_BRIDGE']=$BridgePath
    $info.Environment['AH101_SCOPE']=$scope
    return $info
}
function Invoke-App([string[]]$Arguments){
    $p=[Diagnostics.Process]::Start((New-Info $BridgePath $Arguments))
    try{
        $p.StandardInput.Close()
        $o=$p.StandardOutput.ReadToEndAsync();$e=$p.StandardError.ReadToEndAsync()
        if(-not $p.WaitForExit(5000)){throw 'aHerald command timed out.'}
        if($p.ExitCode -ne 0){throw "aHerald command failed: $($e.Result)"}
        return $o.Result
    }finally{
        if(-not $p.HasExited){$p.Kill($true);$null=$p.WaitForExit(5000)}
        $p.Dispose()
    }
}
function Status { return (Invoke-App @('--status') | ConvertFrom-Json) }
if((Status).result -eq 'status'){throw 'Quit the existing resident before running this isolated acceptance test.'}
# Launch the native bridge directly, without a second recorder shell.
$command='"'+$BridgePath+'" --hook --resident --agent '+$Agent+' --config "'+$config+'"'
if($Agent -eq 'codex'){$command='& '+$command}
$handler=@{type='command';command=$command;timeout=3}
# Keepalive is test scaffolding, not an aHerald hook or a continuation decision.
$keepalive=@{type='command';command='pwsh -NoProfile -NonInteractive -Command "Start-Sleep -Seconds 75"';timeout=85}
$hooks=@{}
foreach($name in @('UserPromptSubmit','PreToolUse','PostToolUse','Stop','SessionEnd')){
    $handlers=@($handler)
    if($name -eq 'Stop' -and -not $Interactive){$handlers+=@($keepalive)}
    $hooks[$name]=@(@{hooks=$handlers})
}
$prompt='Reply with exactly OK. Do not use tools or inspect files.'
if($Agent -eq 'claude'){
    $settings=Join-Path $root 'settings.json'
    @{hooks=$hooks;enabledPlugins=@{}} | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $settings
    $arguments=@('-p',$prompt,'--setting-sources','','--settings',$settings,'--strict-mcp-config','--tools','','--no-session-persistence')
}else{
    $arguments=@('--no-daemon','--dangerously-bypass-hook-trust','--disable','plugins','--disable','apps','--disable','remote_plugin',
        '--disable','plugin_discovery_prefetch','-C',$workspace,'-s','read-only','-c','analytics.enabled=false')
    if($Interactive){$arguments+=@('--no-alt-screen','-p','lean','-a','never')}
    else{$arguments=@('exec','--ignore-user-config','--ignore-rules','--ephemeral','--skip-git-repo-check')+$arguments}
    foreach($name in $hooks.Keys){
        $entries=@()
        foreach($h in $hooks[$name][0].hooks){
            $quoted=$h.command | ConvertTo-Json -Compress
            $entries+=('{type="command",command='+$quoted+',commandWindows='+$quoted+',timeout='+$h.timeout+'}')
        }
        $arguments+=@('-c',('hooks.'+$name+'=[{hooks=['+($entries -join ',')+']}]'))
    }
    $arguments+=@($prompt)
}
$resident=$null;$harness=$null
try{
    $resident=[Diagnostics.Process]::Start((New-Info $AheraldPath @('--resident-child',$scope,'--config',$config)))
    $resident.StandardInput.Close()
    $residentOut=$resident.StandardOutput.ReadToEndAsync();$residentErr=$resident.StandardError.ReadToEndAsync()
    $ready=[Diagnostics.Stopwatch]::StartNew()
    while((Status).result -ne 'status'){
        if($resident.HasExited){throw "Test resident exited $($resident.ExitCode): $($residentErr.Result)"}
        if($ready.Elapsed.TotalSeconds -gt 10){throw 'Test resident readiness timed out.'}
        Start-Sleep -Milliseconds 100
    }
    if((Status).pid -ne $resident.Id){throw 'Another resident won the startup race.'}
    if($Interactive){
        Write-Output "Interactive evidence: $root"
        Write-Output 'Wait for the minute reminder, then submit another prompt to reset it; /quit ends the session.'
        $env:AH101_ROOT=$root;$env:AH101_EXE=$AheraldPath;$env:AH101_AGENT=$Agent
        $env:AH101_BRIDGE=$BridgePath;$env:AH101_SCOPE=$scope
        # Inherit the terminal while retaining ownership and a bounded lifetime.
        $info=New-Info $HarnessPath $arguments
        $info.RedirectStandardInput=$false;$info.RedirectStandardOutput=$false;$info.RedirectStandardError=$false
        $info.CreateNoWindow=$false
        $harness=[Diagnostics.Process]::Start($info)
        if(-not $harness.WaitForExit(180000)){throw 'Interactive harness exceeded 180-second deadline.'}
        $records=@(Get-ChildItem $root -Filter "$Agent-*.json" | ForEach-Object {Get-Content $_.FullName -Raw | ConvertFrom-Json})
        foreach($name in @('UserPromptSubmit','Stop','SessionEnd')){
            if(-not ($records | Where-Object event -eq $name)){throw "No real $name dispatch; evidence: $root"}
        }
        if($records | Where-Object { $_.stage -ne 'completed' -or $_.code -ne 0 -or $_.milliseconds -ge 3000 -or ($_.reply -ne '' -and $_.reply -ne '{}') }){throw 'Hook contract or latency failure.'}
        if((Status).queued -ne 0){throw 'SessionEnd left a stale warning in the queue.'}
        Write-Output "PASS: interactive Codex prompt/Stop/end dispatch and cleanup. Reminder appearance needs separate observation. Evidence: $root"
        return
    }
    $harness=[Diagnostics.Process]::Start((New-Info $HarnessPath $arguments))
    $harness.StandardInput.Close()
    $o=$harness.StandardOutput.ReadToEndAsync();$e=$harness.StandardError.ReadToEndAsync()
    $clock=[Diagnostics.Stopwatch]::StartNew()
    $warned=$false
    while(-not $harness.HasExited -and $clock.Elapsed.TotalSeconds -lt 160){
        $status=Status
        if($status.queued -eq 1 -and -not $warned){
            $warned=$true
            @{agent=$Agent;queued=$status.queued;elapsed_seconds=$clock.Elapsed.TotalSeconds} | ConvertTo-Json -Compress | Set-Content (Join-Path $root 'warning.json')
            Write-Output "WARNING OBSERVED: $Agent; evidence: $root"
        }
        Start-Sleep -Milliseconds 250
    }
    if(-not $harness.HasExited){throw 'Harness exceeded 160-second deadline.'}
    $o.Result | Set-Content (Join-Path $root 'stdout.log')
    $e.Result | Set-Content (Join-Path $root 'stderr.log')
    if($harness.ExitCode -ne 0){throw "Harness failed; inspect $root"}
    $records=@(Get-ChildItem $root -Filter "$Agent-*.json" | ForEach-Object {Get-Content $_.FullName -Raw | ConvertFrom-Json})
    foreach($name in @('UserPromptSubmit','Stop','SessionEnd')){
        if(-not ($records | Where-Object event -eq $name)){throw "No real $name dispatch; evidence: $root"}
    }
    if($records | Where-Object { $_.stage -ne 'completed' -or $_.code -ne 0 -or $_.milliseconds -ge 3000 -or ($_.reply -ne '' -and $_.reply -ne '{}') }){throw 'Hook contract or latency failure.'}
    if(-not $warned){throw "No idle warning was observed; evidence: $root"}
    if((Status).queued -ne 0){throw 'SessionEnd left a stale warning in the queue.'}
    Write-Output "PASS: real $Agent response, prompt/Stop/end hooks, native idle warning and end cleanup. Evidence: $root"
}finally{
    if($null -ne $harness){
        if(-not $harness.HasExited){$harness.Kill($true)}
        $null=$harness.WaitForExit(5000)
        if($null -ne $o){$o.Result | Set-Content (Join-Path $root 'stdout.log')}
        if($null -ne $e){$e.Result | Set-Content (Join-Path $root 'stderr.log')}
        $harness.Dispose()
    }
    if($null -ne $resident){
        if(-not $resident.HasExited){
            if((Status).pid -eq $resident.Id){$null=Invoke-App @('--quit')}
            if(-not $resident.WaitForExit(5000)){$resident.Kill($true);$null=$resident.WaitForExit(5000)}
        }
        if($null -ne $residentOut){$residentOut.Result | Set-Content (Join-Path $root 'resident-stdout.log')}
        if($null -ne $residentErr){$residentErr.Result | Set-Content (Join-Path $root 'resident-stderr.log')}
        $resident.Dispose()
    }
    Write-Output "Evidence: $root"
}

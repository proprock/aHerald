#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$CodexPath,
    [Parameter(Mandatory)][string]$AheraldPath,
    [ValidateRange(3,50)][int]$Samples=9
)
# Opt-in: actual Codex dispatcher, generated binding, process ancestry and timings.
$ErrorActionPreference='Stop'
if(-not $IsWindows){throw 'This probe requires Windows.'}
$CodexPath=(Resolve-Path -LiteralPath $CodexPath).Path
$AheraldPath=(Resolve-Path -LiteralPath $AheraldPath).Path
if([IO.Path]::GetExtension($CodexPath) -ne '.exe'){throw 'Use native codex.exe, not a shell shim.'}
& $CodexPath --version
& $AheraldPath --version | Out-String | Write-Output
$root=Join-Path $env:TEMP ('aherald-hook-launch-'+[guid]::NewGuid().ToString('N'))
$homeDir=Join-Path $root 'home'
$workspace=Join-Path $root 'workspace'
$appDir=Join-Path $root ('Путь space $HOME '+[char]96+'tick & apostrophe'' (round) ! %AHERALD_EXPAND%')
New-Item -ItemType Directory $homeDir,$workspace,$appDir | Out-Null
$exe=Join-Path $appDir 'aherald.exe'
$recorder=Join-Path $root 'recorder.exe'
$record=Join-Path $root 'launch.txt'
& rustc --edition 2024 -O (Join-Path $PSScriptRoot 'fixtures/hook_recorder.rs') -o $recorder
if($LASTEXITCODE -ne 0){throw 'Native recorder compilation failed.'}
function New-Info([string]$File,[string[]]$Arguments){
    $info=[Diagnostics.ProcessStartInfo]::new($File)
    $info.UseShellExecute=$false;$info.CreateNoWindow=$true
    $info.RedirectStandardInput=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
    foreach($argument in $Arguments){$info.ArgumentList.Add($argument)}
    $info.Environment['CODEX_HOME']=$homeDir
    $info.Environment['CLAUDE_CONFIG_DIR']=(Join-Path $root 'claude')
    $info.Environment['AHERALD_LAUNCH_RECORD']=$record
    $info.Environment['AHERALD_EXPAND']='EXPANDED'
    return $info
}
function Invoke-Bounded([Diagnostics.ProcessStartInfo]$Info,[string]$InputText=''){
    $timer=[Diagnostics.Stopwatch]::StartNew()
    $p=[Diagnostics.Process]::Start($Info)
    try{
        $o=$p.StandardOutput.ReadToEndAsync();$e=$p.StandardError.ReadToEndAsync()
        $p.StandardInput.Write($InputText);$p.StandardInput.Close()
        if(-not $p.WaitForExit(10000)){throw 'Command exceeded ten-second deadline.'}
        [pscustomobject]@{Code=$p.ExitCode;Out=$o.Result;Err=$e.Result;Ms=$timer.Elapsed.TotalMilliseconds}
    }finally{
        if(-not $p.HasExited){$p.Kill($true)}
        if(-not $p.WaitForExit(5000)){throw "Process cleanup timed out: $root"}
        $p.Dispose()
    }
}
Copy-Item -LiteralPath $AheraldPath -Destination $exe
$setup=Invoke-Bounded (New-Info $exe @('--setup','--harness','codex','--apply'))
if($setup.Code -ne 0){throw "Setup failed: $($setup.Err)"}
$hooksFile=Join-Path $homeDir 'hooks.json'
$installed=Get-Content -LiteralPath $hooksFile -Raw | ConvertFrom-Json
$handler=$installed.hooks.Stop[0].hooks[0]
if($null -eq $handler){throw 'Setup generated no Stop handler.'}
# Dispatch exactly the generated handler; substitute only the target executable.
@{hooks=@{SessionStart=@(@{hooks=@($handler)})}}|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $hooksFile
Copy-Item -LiteralPath $recorder -Destination $exe -Force
$cliArgs=@('--no-daemon','exec','--ignore-user-config','--ignore-rules','--ephemeral','--skip-git-repo-check',
    '--dangerously-bypass-hook-trust','--disable','plugins','--disable','apps','--disable','remote_plugin',
    '--disable','plugin_discovery_prefetch','-C',$workspace,'-s','read-only','-c','analytics.enabled=false',
    '-c','model_provider="probe"','-c','model_providers.probe.name="probe"',
    '-c','model_providers.probe.base_url="http://127.0.0.1:9/v1"',
    '-c','model_providers.probe.wire_api="responses"','-c','model_providers.probe.requires_openai_auth=false',
    '-c','model_providers.probe.request_max_retries=0','-c','model_providers.probe.stream_max_retries=0',
    '-m','probe','Do not execute tools.')
$p=$null
try{
    $p=[Diagnostics.Process]::Start((New-Info $CodexPath $cliArgs));$p.StandardInput.Close()
    $o=$p.StandardOutput.ReadToEndAsync();$e=$p.StandardError.ReadToEndAsync()
    $timer=[Diagnostics.Stopwatch]::StartNew()
    while($timer.Elapsed.TotalSeconds -lt 25 -and -not $p.HasExited){
        if(Test-Path -LiteralPath $record){break}
        Start-Sleep -Milliseconds 50
    }
}finally{
    if($null -ne $p){
        if(-not $p.HasExited){$p.Kill($true)}
        if(-not $p.WaitForExit(5000)){throw "Codex cleanup timed out: $root"}
        $o.Result|Set-Content -LiteralPath (Join-Path $root 'stdout.log')
        $e.Result|Set-Content -LiteralPath (Join-Path $root 'stderr.log')
        $p.Dispose()
    }
    Write-Output "Evidence: $root"
}
$observed=@{}
foreach($line in Get-Content -LiteralPath $record){$pair=$line.Split('=',2);$observed[$pair[0]]=$pair[1]}
Get-Content -LiteralPath $record
if($observed.arg0 -ne '--hook' -or $observed.arg1 -ne '--resident' -or $observed.ContainsKey('arg2')){
    throw 'Generated binding did not preserve exact arguments.'
}
if([int]$observed.stdin_bytes -le 0){throw 'Codex did not supply hook input.'}
if($observed.parent1 -notin @('pwsh.exe','powershell.exe') -or $observed.parent2 -ne 'codex.exe'){
    throw 'Generated binding added a launcher between Codex shell and native executable.'
}
# Restore the GUI executable; disable delivery to prevent residents and cards.
Copy-Item -LiteralPath $AheraldPath -Destination $exe -Force
@('[codex]','hooks_enabled=false')|Set-Content -LiteralPath (Join-Path $appDir 'config.toml')
$inputText=Get-Content -LiteralPath (Join-Path $PSScriptRoot '../fixtures/stop.json') -Raw
$direct=$handler.commandWindows
$oldScript='$ProgressPreference=''SilentlyContinue''; & '''+$exe.Replace("'","''")+''' --hook --resident | Out-String'
$encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($oldScript))
$previous='powershell.exe -NoProfile -NonInteractive -OutputFormat Text -EncodedCommand '+$encoded
$results=@()
foreach($shell in @((Get-Command pwsh).Source,(Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'))|Select-Object -Unique){
    foreach($iteration in 0..($Samples+1)){
        $variants=@(@{Name='direct';Command=$direct},@{Name='extra-powershell';Command=$previous})
        if($iteration % 2 -eq 1){[array]::Reverse($variants)}
        foreach($variant in $variants){
            $result=Invoke-Bounded (New-Info $shell @('-NoProfile','-NonInteractive','-Command',$variant.Command)) $inputText
            if($result.Code -ne 0 -or $result.Out.Trim() -ne '{}' -or $result.Err.Trim() -ne ''){
                throw "Hook response failed in $shell / $($variant.Name): code=$($result.Code), out=$($result.Out), err=$($result.Err)"
            }
            if($iteration -ge 2){$results += [pscustomobject]@{Shell=[IO.Path]::GetFileName($shell);Variant=$variant.Name;Ms=$result.Ms}}
        }
    }
}
# Native process baseline separates executable work from shell startup.
foreach($iteration in 0..($Samples+1)){
    $result=Invoke-Bounded (New-Info $exe @('--hook','--resident')) $inputText
    if($result.Code -ne 0 -or $result.Out.Trim() -ne '{}' -or $result.Err.Trim() -ne ''){throw 'Native baseline response failed.'}
    if($iteration -ge 2){$results += [pscustomobject]@{Shell='none';Variant='native-exe';Ms=$result.Ms}}
}
$results|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $root 'timings.json')
$results|Group-Object Shell,Variant|ForEach-Object{
    $sorted=@($_.Group.Ms|Sort-Object)
    [pscustomobject]@{Case=$_.Name;Samples=$sorted.Count;MedianMs=[Math]::Round($sorted[[int][Math]::Floor($sorted.Count/2)],1);MeanMs=[Math]::Round(($_.Group.Ms|Measure-Object -Average).Average,1);MaxMs=[Math]::Round($sorted[-1],1)}
}|Format-Table|Out-String|Write-Output
Write-Output 'PASS: generated real-Codex hook has no added launcher; real aHerald responses passed in PowerShell 7 and Windows PowerShell 5.1.'

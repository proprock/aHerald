param()
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
$root = Join-Path ([IO.Path]::GetTempPath()) ('aherald-runner-probe-' + [guid]::NewGuid().ToString('N'))
$previousRoot = $env:AH_RUNNER_PROBE_ROOT
$rootCreated = $false
$runner = $null
$observed = @()
try {
    New-Item -ItemType Directory -Path $root | Out-Null
    $rootCreated = $true
    New-Item -ItemType Directory -Force -Path target/ah109 | Out-Null
    $env:AH_RUNNER_PROBE_ROOT = $root
    # Fault injection: each missing scheduling rule must cause real contention.
    $redConfig = Join-Path $root 'unscheduled.toml'
    @'
[profile.default]
test-threads = 2
retries = 0
fail-fast = false
slow-timeout = { period = "5s", terminate-after = 2 }
'@ | Set-Content $redConfig -Encoding utf8
    & cargo nextest --config-file $redConfig run --test runner_contract --run-ignored only -E 'test(/desktop_slot_/)' *> target/ah109/group-red.log
    if ($LASTEXITCODE -ne 100 -or (Get-Content target/ah109/group-red.log -Raw) -notmatch 'AlreadyExists') { throw 'Missing desktop group did not produce expected contention' }
    @'
[test-groups]
desktop = { max-threads = 1 }
isolated = { max-threads = 1 }
[profile.default]
test-threads = 2
retries = 0
fail-fast = false
slow-timeout = { period = "5s", terminate-after = 2 }
[[profile.default.overrides]]
filter = 'test(=background_slot)'
test-group = "isolated"
[[profile.default.overrides]]
filter = 'all()'
test-group = "desktop"
'@ | Set-Content $redConfig -Encoding utf8
    & cargo nextest --config-file $redConfig run --test runner_contract --run-ignored only -E 'test(/performance_slot|background_slot/)' *> target/ah109/performance-red.log
    if ($LASTEXITCODE -ne 100 -or (Get-Content target/ah109/performance-red.log -Raw) -notmatch 'AlreadyExists') { throw 'Missing performance exclusivity did not produce expected contention' }

    & cargo nextest run --profile runner-probe --test runner_contract --run-ignored only -E 'test(/desktop_slot_/)'  *> target/ah109/group-green.log
    if ($LASTEXITCODE -ne 0) { throw 'Desktop group did not serialize the two resource probes' }
    & cargo nextest run --profile runner-probe --test runner_contract --run-ignored only -E 'test(/performance_slot|background_slot/)' *> target/ah109/performance-green.log
    if ($LASTEXITCODE -ne 0) { throw 'Performance test overlapped a background workload' }
    $nextest = (Get-Command cargo-nextest).Source
    $runner = Start-Process -FilePath $nextest -ArgumentList @('nextest','run','--profile','runner-probe','--test','runner_contract','--run-ignored','only','-E','test(=timeout_owns_a_detached_resident)') -WorkingDirectory $repo -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $repo 'target/ah109/timeout.stdout.log') -RedirectStandardError (Join-Path $repo 'target/ah109/timeout.stderr.log')
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (-not (Test-Path -LiteralPath (Join-Path $root 'ready.json'))) {
        if ($runner.HasExited) { throw 'Timeout probe exited before resident readiness' }
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Timeout probe did not publish readiness in 30 seconds' }
        Start-Sleep -Milliseconds 10
    }
    $record = Get-Content -LiteralPath (Join-Path $root 'ready.json') -Raw | ConvertFrom-Json
    foreach ($ownedId in @($record.pid, $record.test_pid)) {
        $process = Get-Process -Id $ownedId
        # Force acquisition of the exact native process handle before runner termination.
        $null = $process.Handle
        $observed += $process
    }
    if (-not $runner.WaitForExit(20000)) { throw 'Runner failed to enforce its five-second test timeout' }
    if ($runner.ExitCode -ne 100) { throw "Expected test-failure exit 100, observed $($runner.ExitCode)" }
    foreach ($process in $observed) {
        if (-not $process.WaitForExit(3000)) { throw 'Nextest timeout left an owned test/resident process alive' }
    }
    $timeoutLog = Get-Content target/ah109/timeout.stderr.log -Raw
    if ($timeoutLog -notmatch 'TIMEOUT') { throw 'Runner did not classify the hung test as TIMEOUT' }
    Copy-Item target/nextest/runner-probe/junit.xml target/ah109/timeout-junit.xml
    [xml]$report = Get-Content target/ah109/timeout-junit.xml -Raw
    if (-not ($report.SelectSingleNode('//testcase/error') -or $report.SelectSingleNode('//testcase/failure'))) { throw 'JUnit omitted the timeout failure' }
    & cargo nextest run --profile runner-probe --test runner_contract --run-ignored only -E 'test(=timeout_released_native_resources)' *> target/ah109/cleanup-green.log
    if ($LASTEXITCODE -ne 0) { throw 'Killed resident retained native IPC/file resources' }
    Write-Output 'PASS: desktop serialization, performance exclusivity, TIMEOUT exit/report, exact test/resident exit, mutex/pipe/file release'
} finally {
    if ($runner -and -not $runner.HasExited) { $runner.Kill($true); if (-not $runner.WaitForExit(5000)) { throw 'Runner cleanup exceeded deadline' } }
    foreach ($process in $observed) {
        if (-not $process.HasExited) { $process.Kill(); $null = $process.WaitForExit(3000) }
        $process.Dispose()
    }
    if ($runner) { $runner.Dispose() }
    $env:AH_RUNNER_PROBE_ROOT = $previousRoot
    # Check the resolved deletion target remains the exact private temporary directory.
    $resolved = [IO.Path]::GetFullPath($root)
    $temp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if (-not $resolved.StartsWith($temp, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolved) -notmatch '^aherald-runner-probe-[0-9a-f]{32}$') { throw 'Unexpected cleanup path' }
    if ($rootCreated -and (Test-Path -LiteralPath $resolved)) { Remove-Item -LiteralPath $resolved -Recurse -Force }
    Pop-Location
}

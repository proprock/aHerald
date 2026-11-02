param([ValidateRange(2, 10)][int]$Runs = 3, [switch]$RebuildPackage, [switch]$Resume)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
$results = [Collections.Generic.List[object]]::new()
$directory = Join-Path $repo 'target/ah109'
New-Item -ItemType Directory -Force -Path $directory | Out-Null
function Measure-Runner([string]$Name, [string]$Phase, [int]$Round, [string[]]$Arguments) {
    $log = Join-Path $directory "$Name-$Phase-$Round.log"
    $clock = [Diagnostics.Stopwatch]::StartNew()
    & cargo @Arguments *> $log
    $exitCode = $LASTEXITCODE
    $clock.Stop()
    $row = [ordered]@{ runner = $Name; phase = $Phase; round = $Round; seconds = $clock.Elapsed.TotalSeconds; exit = $exitCode; command = 'cargo ' + ($Arguments -join ' '); log = [IO.Path]::GetFileName($log) }
    if ($Phase -eq 'run') {
        if ($Name -eq 'cargo') {
            $counts = [regex]::Matches((Get-Content $log -Raw), 'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;')
            $row['summary_passed'] = ($counts | ForEach-Object { [int]$_.Groups[1].Value } | Measure-Object -Sum).Sum
            $row['summary_failed'] = ($counts | ForEach-Object { [int]$_.Groups[2].Value } | Measure-Object -Sum).Sum
            $row['summary_ignored'] = ($counts | ForEach-Object { [int]$_.Groups[3].Value } | Measure-Object -Sum).Sum
        } else {
            $reportPath = Join-Path $directory "nextest-run-$Round.xml"
            Copy-Item target/nextest/ci/junit.xml $reportPath
            [xml]$report = Get-Content $reportPath -Raw
            $row['summary_tests'] = [int]$report.testsuites.tests
            $row['summary_failed'] = [int]$report.testsuites.failures + [int]$report.testsuites.errors
            $row['junit_skipped'] = [int]$report.testsuites.skipped
            # Nextest excludes ignored cases from JUnit; retain selection counts separately.
            $row['summary_ignored'] = [int](Get-Content (Join-Path $directory 'coverage.json') -Raw | ConvertFrom-Json).ignored
        }
    }
    $results.Add([pscustomobject]$row)
    $results | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $directory 'measurements.json') -Encoding utf8
    Write-Output "$Name $Phase $Round : $([Math]::Round($clock.Elapsed.TotalSeconds,3)) s, exit=$exitCode"
    if ($exitCode -ne 0) { throw "Failed $Name $Phase; inspect $log before another run" }
}
try {
    $version = & cargo nextest --version
    if ($LASTEXITCODE -ne 0 -or ($version -join "`n") -notmatch '^cargo-nextest 0\.9\.150(?:\s|$)') { throw 'Install pinned nextest 0.9.150 first' }
    if ($Resume) {
        Get-Content (Join-Path $directory 'measurements.json') -Raw | ConvertFrom-Json | ForEach-Object { $results.Add($_) }
        if ($results | Where-Object phase -eq 'run') { throw 'Resume is only for an inventory failure before execution rounds; preserve and inspect existing run evidence' }
    } else {
        [ordered]@{ nextest = $version; rustc = (& rustc --version); cargo = (& cargo --version); commit = (& git rev-parse HEAD); os = [Environment]::OSVersion.VersionString; processors = [Environment]::ProcessorCount; package_cold = [bool]$RebuildPackage; runs = $Runs; started_utc = [DateTime]::UtcNow.ToString('o') } | ConvertTo-Json | Set-Content (Join-Path $directory 'environment.json') -Encoding utf8
        foreach ($name in @('cargo', 'nextest')) {
            if ($RebuildPackage) {
                # Standard target/cache only: dependencies remain warm for BOTH builds.
                & cargo clean -p aherald --profile test *> (Join-Path $directory "$name-clean.log")
                if ($LASTEXITCODE -ne 0) { throw 'Package clean failed' }
            }
            $arguments = if ($name -eq 'cargo') { @('test','--all-targets','--no-run','--locked') } else { @('nextest','run','--all-targets','--no-run','--locked') }
            Measure-Runner $name 'build' 1 $arguments
        }
    }
    # Compare actual libtest listings (including ignored flags) with nextest's inventory.
    & cargo nextest list --all-targets --message-format json > (Join-Path $directory 'inventory.json')
    if ($LASTEXITCODE -ne 0) { throw 'Nextest inventory failed' }
    $inventory = Get-Content (Join-Path $directory 'inventory.json') -Raw | ConvertFrom-Json
    $ordinary = 0; $ignored = 0
    foreach ($property in $inventory.'rust-suites'.PSObject.Properties) {
        $suite = $property.Value
        $listed = @(& $suite.'binary-path' --list --format terse | Where-Object { $_ -match ': test$' } | ForEach-Object { $_ -replace ': test$', '' })
        if ($LASTEXITCODE -ne 0) { throw "Libtest inventory failed: $($suite.'binary-id')" }
        $expected = @($suite.testcases.PSObject.Properties | ForEach-Object Name)
        if ($listed.Count -ne $expected.Count -or ($expected.Count -and (Compare-Object ($listed | Sort-Object) ($expected | Sort-Object)))) { throw "Runner coverage differs: $($suite.'binary-id')" }
        $listedIgnored = @(& $suite.'binary-path' --list --ignored --format terse | Where-Object { $_ -match ': test$' } | ForEach-Object { $_ -replace ': test$', '' })
        if ($LASTEXITCODE -ne 0) { throw 'Ignored libtest inventory failed' }
        $expectedIgnored = @($suite.testcases.PSObject.Properties | Where-Object { $_.Value.ignored } | ForEach-Object Name)
        if ($listedIgnored.Count -ne $expectedIgnored.Count -or ($expectedIgnored.Count -and (Compare-Object ($listedIgnored | Sort-Object) ($expectedIgnored | Sort-Object)))) { throw "Ignored coverage differs: $($suite.'binary-id')" }
        $ordinary += $expected.Count - $expectedIgnored.Count
        $ignored += $expectedIgnored.Count
    }
    [ordered]@{ ordinary = $ordinary; ignored = $ignored; total = $inventory.'test-count'; binaries = $inventory.'rust-suites'.PSObject.Properties.Name.Count } | ConvertTo-Json | Set-Content (Join-Path $directory 'coverage.json') -Encoding utf8
    Write-Output "Matching runner inventory: $ordinary ordinary, $ignored ignored"
    for ($round = 1; $round -le $Runs; $round++) {
        $order = if ($round % 2) { @('cargo','nextest') } else { @('nextest','cargo') }
        foreach ($name in $order) {
            $arguments = if ($name -eq 'cargo') { @('test','--all-targets','--locked') } else { @('nextest','run','--all-targets','--locked','--profile','ci') }
            Measure-Runner $name 'run' $round $arguments
        }
    }
    & cargo test --doc --locked *> (Join-Path $directory 'doctests.log')
    if ($LASTEXITCODE -ne 0) { throw 'Doctest coverage failed' }
} finally { Pop-Location }

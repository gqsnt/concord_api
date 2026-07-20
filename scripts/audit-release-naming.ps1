[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$root = (& git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($root)) {
    throw "unable to resolve the Git worktree root"
}
Set-Location -LiteralPath $root

$scope = @(
    "concord_core",
    "concord_macros",
    "concord_test_support",
    "concord_examples",
    "perf",
    ".config",
    "Cargo.toml",
    "justfile"
)
$tracked = @(& git ls-files -- @scope)
if ($LASTEXITCODE -ne 0) {
    throw "git ls-files failed"
}

$chronology = [regex]'(?i)(?:^|[^a-z])phase[ _-]?[0-9]+(?:[^a-z0-9]|$)'
$relativeChronology = [regex]'(?i)\b(?:earlier|later|current|next|previous)[ _-]?phase(?:s|[ _-](?:boundary|work|implementation))?\b'
$testIdentifier = [regex]'(?i)\b(?:async\s+)?fn\s+phase_?[0-9]+'
$violations = [System.Collections.Generic.List[string]]::new()

foreach ($path in $tracked) {
    if ($chronology.IsMatch($path.Replace('\', '/'))) {
        $violations.Add("tracked executable path: $path")
    }
    $absolute = Join-Path $root $path
    if (-not (Test-Path -LiteralPath $absolute -PathType Leaf)) {
        continue
    }
    $content = Get-Content -LiteralPath $absolute -Raw
    if ($chronology.IsMatch($content)) {
        $violations.Add("development chronology in executable content: $path")
    }
    if ($relativeChronology.IsMatch($content)) {
        $violations.Add("relative development chronology in executable content: $path")
    }
    if ($testIdentifier.IsMatch($content)) {
        $violations.Add("development chronology in test identifier: $path")
    }
}

$requiredSuites = @(
    "concord_macros/tests/generated_request_shaping.rs",
    "concord_macros/tests/generated_authentication.rs",
    "concord_macros/tests/generated_rate_limiting.rs",
    "concord_macros/tests/generated_pagination.rs",
    "concord_macros/tests/generated_prepared_endpoints.rs"
)
foreach ($path in $requiredSuites) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $path) -PathType Leaf)) {
        $violations.Add("missing behavior-oriented test suite: $path")
    }
}

$failureFixtures = Get-ChildItem -LiteralPath (Join-Path $root "concord_macros/tests/ui/fail") -Filter "*.rs" -File
foreach ($fixture in $failureFixtures) {
    $stderr = [System.IO.Path]::ChangeExtension($fixture.FullName, ".stderr")
    if (-not (Test-Path -LiteralPath $stderr -PathType Leaf)) {
        $violations.Add("Trybuild fixture has no matching stderr: $($fixture.FullName)")
    }
}
$orphanStderr = Get-ChildItem -LiteralPath (Join-Path $root "concord_macros/tests/ui/fail") -Filter "*.stderr" -File |
    Where-Object { -not (Test-Path -LiteralPath ([System.IO.Path]::ChangeExtension($_.FullName, ".rs")) -PathType Leaf) }
foreach ($fixture in $orphanStderr) {
    $violations.Add("Trybuild stderr has no matching source: $($fixture.FullName)")
}

if ($violations.Count -ne 0) {
    $violations | ForEach-Object { Write-Error $_ }
    throw "release naming audit found $($violations.Count) violation(s)"
}

Write-Output "release naming audit passed: $($tracked.Count) tracked executable files and $($failureFixtures.Count) Trybuild pairs"

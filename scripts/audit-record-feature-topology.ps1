[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$root = (& git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($root)) {
    throw "unable to resolve the Git worktree root"
}
Set-Location -LiteralPath $root

function Cargo-Tree {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    $output = (& cargo tree --offline --locked @Arguments 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0) {
        throw "cargo tree failed for arguments '$($Arguments -join ' ')':`n$output"
    }
    return $output
}

function Require-Match {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string]$Text,
        [Parameter(Mandatory = $true)][string]$Pattern
    )
    if ($Text -notmatch $Pattern) {
        throw "$Label omitted required pattern: $Pattern"
    }
}

function Reject-Match {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string]$Text,
        [Parameter(Mandatory = $true)][string]$Pattern
    )
    if ($Text -match $Pattern) {
        throw "$Label unexpectedly contained pattern: $Pattern"
    }
}

$normalBase = @("-p", "concord_core", "-e", "normal", "--prefix", "none")
$default = Cargo-Tree -Arguments $normalBase
$records = Cargo-Tree -Arguments ($normalBase + @("--no-default-features", "--features", "records"))
$csv = Cargo-Tree -Arguments ($normalBase + @("--no-default-features", "--features", "csv"))
$ndjson = Cargo-Tree -Arguments ($normalBase + @("--no-default-features", "--features", "ndjson"))
$combined = Cargo-Tree -Arguments ($normalBase + @("--no-default-features", "--features", "csv,ndjson"))

foreach ($graph in @(
    @{ Label = "default graph"; Text = $default },
    @{ Label = "records graph"; Text = $records }
)) {
    Reject-Match $graph.Label $graph.Text '(?m)^csv v'
    Reject-Match $graph.Label $graph.Text '(?m)^serde v'
    Reject-Match $graph.Label $graph.Text '(?m)^serde_json v'
}

Require-Match "csv graph" $csv '(?m)^csv v1\.3\.1\r?$'
Require-Match "csv graph" $csv '(?m)^serde v1\.0\.228(?: \(\*\))?\r?$'
Reject-Match "csv graph" $csv '(?m)^serde_json v'
Require-Match "ndjson graph" $ndjson '(?m)^serde v1\.0\.228(?: \(\*\))?\r?$'
Require-Match "ndjson graph" $ndjson '(?m)^serde_json v1\.0\.150\r?$'
Reject-Match "ndjson graph" $ndjson '(?m)^csv v'
Require-Match "combined graph" $combined '(?m)^csv v1\.3\.1\r?$'
Require-Match "combined graph" $combined '(?m)^serde_json v1\.0\.150\r?$'

$csvFeatures = Cargo-Tree -Arguments @(
    "-p", "concord_core", "--no-default-features", "--features", "csv",
    "-e", "features", "-i", "concord_core"
)
$ndjsonFeatures = Cargo-Tree -Arguments @(
    "-p", "concord_core", "--no-default-features", "--features", "ndjson",
    "-e", "features", "-i", "concord_core"
)
Require-Match "csv feature closure" $csvFeatures 'concord_core feature "csv"'
Require-Match "csv feature closure" $csvFeatures 'concord_core feature "records"'
Reject-Match "csv feature closure" $csvFeatures 'concord_core feature "json"'
Reject-Match "csv feature closure" $csvFeatures 'concord_core feature "ndjson"'
Require-Match "ndjson feature closure" $ndjsonFeatures 'concord_core feature "ndjson"'
Require-Match "ndjson feature closure" $ndjsonFeatures 'concord_core feature "records"'
Reject-Match "ndjson feature closure" $ndjsonFeatures 'concord_core feature "json"'
Reject-Match "ndjson feature closure" $ndjsonFeatures 'concord_core feature "csv"'

$workspaceManifest = Get-Content -LiteralPath "Cargo.toml" -Raw
$coreManifest = Get-Content -LiteralPath "concord_core/Cargo.toml" -Raw
$macroManifest = Get-Content -LiteralPath "concord_macros/Cargo.toml" -Raw
$lockfile = Get-Content -LiteralPath "Cargo.lock" -Raw
Require-Match "workspace manifest" $workspaceManifest 'csv = "=1\.3\.1"'
Require-Match "core manifest" $coreManifest 'records = \[\]'
Require-Match "core manifest" $coreManifest 'csv = \["records", "dep:csv", "dep:serde"\]'
Require-Match "core manifest" $coreManifest 'ndjson = \["records", "dep:serde", "dep:serde_json"\]'
Require-Match "core manifest" $coreManifest 'csv = \{ workspace = true, optional = true \}'
Require-Match "lockfile" $lockfile 'name = "csv"\r?\nversion = "1\.3\.1"'
foreach ($forbidden in @("csv", "serde_json", "records")) {
    Reject-Match "concord_macros manifest" $macroManifest "(?m)^$forbidden\\s*="
}

Write-Output "record feature topology audit passed: default isolated; records dependency-free; csv=1.3.1+serde; ndjson=serde+serde_json; macro graph isolated"

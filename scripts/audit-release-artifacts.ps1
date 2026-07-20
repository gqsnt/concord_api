[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$root = (& git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($root)) {
    throw "unable to resolve the Git worktree root"
}
Set-Location -LiteralPath $root

function Require-Marker {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string[]]$Markers
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "required release artifact is absent: $Path"
    }
    $content = Get-Content -LiteralPath $Path -Raw
    foreach ($marker in $Markers) {
        if (-not $content.Contains($marker)) {
            throw "release artifact $Path is missing marker: $marker"
        }
    }
}

Require-Marker "Cargo.toml" @('"concord_examples"', '"perf"', 'rust-version = "1.85"', 'reqwest = { version = "=0.13.4", default-features = false }')
Require-Marker "rust-toolchain.toml" @('channel = "1.85.0"')
Require-Marker "concord_core/Cargo.toml" @('dangerous-raw-response = []', 'dangerous-custom-retry = []', 'reqwest = { workspace = true, features = ["stream"] }')
Require-Marker "concord_examples/Cargo.toml" @('publish = false', 'dangerous-raw-response = ["concord_core/dangerous-raw-response"]', 'dangerous-custom-retry = ["concord_core/dangerous-custom-retry", "dep:reqwest"]')
Require-Marker "perf/Cargo.toml" @('name = "concord_perf"', 'publish = false', 'harness = false')

$examples = @{
    "concord_examples/src/main.rs" = @("client MinimalApi")
    "concord_examples/examples/full_dsl_reference.rs" = @("client ReferenceApi", "retry reqwest_default")
    "concord_examples/examples/body_workflows.rs" = @("JsonRoundTrip", "TextRoundTrip", "CustomRoundTrip", "Upload", "Download", "MultipartUpload")
    "concord_examples/examples/authentication.rs" = @("oauth2_client", "endpoint Login", "custom concord_examples::ExampleToken")
    "concord_examples/examples/rate_limiting.rs" = @("RetryAfterObserver", "rate_limits")
    "concord_examples/examples/pagination.rs" = @("OffsetPagination", "PagePagination", "CursorPagination", "paginate custom")
    "concord_examples/examples/prepared_endpoint.rs" = @("PreparedEndpoint::builder")
    "concord_examples/examples/dangerous_raw_response.rs" = @("RawResponseRequest", ".raw_response()")
    "concord_examples/examples/dangerous_custom_retry.rs" = @("ReqwestRetryBuilder", "retry custom")
    "concord_examples/examples/regional_api.rs" = @("client RegionalGameApi", "scope Accounts", "scope Matches", "scope League")
}
foreach ($entry in $examples.GetEnumerator()) {
    Require-Marker $entry.Key $entry.Value
}

Require-Marker "perf/benches/concord_overhead.rs" @(
    '"core client construction"',
    '"generated client, facade, and request creation"',
    '"public head preparation"',
    '"codec encoding and decoding"',
    '"body-limit polling"',
    '"credential ready-path preparation"',
    '"limiter ready-path acquisition"',
    '"pagination loop overhead"',
    '"stream wrapping"'
)

$grammarRows = (Get-Content -LiteralPath "concord_macros/tests/release/grammar_registry.csv").Count - 1
$reqwestRows = (Get-Content -LiteralPath "concord_core/tests/release/reqwest_assumptions.csv").Count - 1
$securityRows = (Get-Content -LiteralPath "concord_core/tests/release/security_sentinels.csv").Count - 1
$propertyRows = (Get-Content -LiteralPath "concord_core/tests/release/properties.csv").Count - 1
if ($grammarRows -ne 68 -or $reqwestRows -ne 18 -or $securityRows -ne 15 -or $propertyRows -ne 5) {
    throw "release registry counts differ: grammar=$grammarRows reqwest=$reqwestRows security=$securityRows properties=$propertyRows"
}

$metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {
    throw "cargo metadata failed"
}
$packageNames = @($metadata.packages | ForEach-Object name)
foreach ($package in @("concord_core", "concord_macros", "concord_test_support", "concord_examples", "concord_perf")) {
    if ($packageNames -notcontains $package) {
        throw "workspace metadata omitted $package"
    }
}

Write-Output "release artifact audit passed: examples=$($examples.Count) benchmark-categories=9 grammar=$grammarRows reqwest=$reqwestRows security=$securityRows properties=$propertyRows"

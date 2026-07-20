[CmdletBinding()]
param(
    [ValidateSet("Source", "Final")]
    [string]$Mode = "Source",
    [string]$SourceCommit = "",
    [string]$HistoricalBaseline = "e05008c1ebc88e04f4df1001ecce00ae888282a3"
)

$ErrorActionPreference = "Stop"
$root = (& git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($root)) {
    throw "unable to resolve the Git worktree root"
}
Set-Location -LiteralPath $root

$coveragePath = "docs/conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv"
$evidencePath = "docs/conformance/CONFORMANCE_EVIDENCE_0.1.csv"
$coverage = @(Import-Csv -LiteralPath $coveragePath)
$evidence = @(Import-Csv -LiteralPath $evidencePath)

if ($coverage.Count -ne 504) {
    throw "coverage registry contains $($coverage.Count) rows rather than 504"
}
$coverageIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($row in $coverage) {
    if (-not $coverageIds.Add($row.requirement_id)) {
        throw "duplicate coverage requirement: $($row.requirement_id)"
    }
}

$normativeIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$normativeFiles = Get-ChildItem -LiteralPath "docs/normative" -Filter "0*_CONCORD_*_0.1.md" -File
foreach ($file in $normativeFiles) {
    $content = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($match in [regex]::Matches($content, '\*\*([A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+-[0-9]{3})\s+—')) {
        [void]$normativeIds.Add($match.Groups[1].Value)
    }
}
if ($normativeIds.Count -ne 504) {
    throw "normative owner set contains $($normativeIds.Count) IDs rather than 504"
}
foreach ($id in $coverageIds) {
    if (-not $normativeIds.Contains($id)) {
        throw "coverage ID has no normative owner: $id"
    }
}
foreach ($id in $normativeIds) {
    if (-not $coverageIds.Contains($id)) {
        throw "normative ID is absent from coverage: $id"
    }
}

$allowedLabels = [System.Collections.Generic.HashSet[string]]::new(
    [string[]]@(
        "parser-unit", "semantic-unit", "compile-pass", "compile-fail", "downstream-generated",
        "downstream-extension", "runtime", "concurrency", "model-check", "property", "fuzz",
        "feature-boundary", "msrv", "doc-test", "manual-review", "release-command",
        "artifact-audit", "boundary-test", "catalog-audit", "compatibility-matrix",
        "compile-fail-and-rustdoc", "compile-fail-runtime-and-rustdoc", "configuration",
        "conformance", "doctest-and-rustdoc", "downstream-generated-runtime", "feature-matrix",
        "lifecycle-runtime", "manifest", "parser-semantic", "public-api", "runtime-and-compile",
        "rustdoc", "security-runtime", "source-runtime"
    ),
    [System.StringComparer]::Ordinal
)
$evidenceIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($row in $evidence) {
    if (-not $coverageIds.Contains($row.requirement_id)) {
        throw "evidence row $($row.evidence_id) has unknown requirement $($row.requirement_id)"
    }
    if (-not $evidenceIds.Add($row.evidence_id)) {
        throw "duplicate evidence ID: $($row.evidence_id)"
    }
    if (-not $allowedLabels.Contains($row.evidence_type)) {
        throw "unapproved evidence label $($row.evidence_type) in $($row.evidence_id)"
    }
    if ($row.result -notin @("passing", "waived")) {
        throw "invalid result in $($row.evidence_id): $($row.result)"
    }
    if ($row.result -eq "waived" -or -not [string]::IsNullOrWhiteSpace($row.waiver)) {
        throw "unapproved waiver in $($row.evidence_id)"
    }
}

$historicalDiff = @(& git diff --unified=0 $HistoricalBaseline -- $evidencePath)
if ($LASTEXITCODE -ne 0) {
    throw "historical evidence diff failed"
}
$removedHistoricalLines = @($historicalDiff | Where-Object { $_ -match '^-(?!--)' })
if ($removedHistoricalLines.Count -ne 0) {
    throw "historical evidence rows were removed or rewritten"
}

if ($Mode -eq "Final") {
    if ($SourceCommit -notmatch '^[0-9a-f]{40}$') {
        throw "Final mode requires the exact 40-character tested source commit"
    }
    $releaseRows = @($evidence | Where-Object { $_.evidence_id -like "P10-*" })
    if ($releaseRows.Count -eq 0) {
        throw "Final mode found no Phase 10 evidence rows"
    }
    foreach ($row in $releaseRows) {
        if ($row.configuration -notmatch '^direct ') {
            throw "release evidence is not direct: $($row.evidence_id)"
        }
        if ($row.result -ne "passing") {
            throw "release evidence did not pass: $($row.evidence_id)"
        }
        if ($row.verified_commit -ne $SourceCommit) {
            throw "release evidence $($row.evidence_id) does not bind tested source $SourceCommit"
        }
    }
    $directIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $evidence | Where-Object { $_.result -eq "passing" -and $_.configuration -match '^direct ' }) {
        [void]$directIds.Add($row.requirement_id)
    }
    $missing = @($coverageIds | Where-Object { -not $directIds.Contains($_) } | Sort-Object)
    if ($missing.Count -ne 0) {
        throw "requirements without direct passing evidence: $($missing -join ', ')"
    }
    foreach ($file in $normativeFiles) {
        $content = Get-Content -LiteralPath $file.FullName -Raw
        if (-not $content.Contains('**Status:** Normative Release Final')) {
            throw "normative document is not release-final: $($file.Name)"
        }
    }
}

Write-Output "release registry audit passed: mode=$Mode requirements=$($coverage.Count) normative=$($normativeIds.Count) evidence=$($evidence.Count) labels=$($allowedLabels.Count) waivers=0"

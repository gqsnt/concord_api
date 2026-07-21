[CmdletBinding()]
param(
    [ValidateSet("SelfTest", "Source", "Evidence", "Final")]
    [string]$Mode = "Source",
    [string]$SourceCommit = "",
    [string]$EvidenceCommit = "",
    [string]$AnchorCommit = "",
    [string]$MetadataCommit = "",
    [string]$PhaseBaseline = "",
    [string]$CorrectionBaseline = "44799306fbf1bd039b73c5e57903aed30e5e7a0d",
    [string]$HistoricalBaseline = "e05008c1ebc88e04f4df1001ecce00ae888282a3"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$script:EvidenceToken = "EVIDENCE_COMMIT_PENDING"
$script:AnchorToken = "ANCHOR_COMMIT_PENDING"
$script:EvidencePath = "docs/conformance/CONFORMANCE_EVIDENCE_0.1.csv"
$script:CoveragePath = "docs/conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv"
$script:RequirementRegistryPath = "docs/conformance/RELEASE_REQUIREMENT_REGISTRY_0.1.csv"
$script:ClaimRegistryPath = "docs/conformance/RELEASE_CLAIM_REGISTRY_0.1.csv"
$script:ChangeMapPath = "docs/conformance/RELEASE_CHANGE_MAP_0.1.csv"
$script:LanguageAllowlistPath = "docs/conformance/NORMATIVE_LANGUAGE_ALLOWLIST_0.1.csv"
$script:DependencyRegistryPath = "docs/conformance/NORMATIVE_DEPENDENCY_REGISTRY_0.1.csv"
$script:OwnershipRegistryPath = "docs/conformance/NORMATIVE_OWNERSHIP_REGISTRY_0.1.csv"
$script:ValidationRegistryPath = "docs/conformance/RELEASE_VALIDATION_REGISTRY_0.1.csv"
$script:RequirementChangeRecordPath = "docs/normative/11_CONCORD_REQUIREMENT_CHANGE_RECORD_0.4.md"
$script:PriorRequirementChangeRecordPath = "docs/normative/10_CONCORD_REQUIREMENT_CHANGE_RECORD_0.3.md"
$script:Phase11RequirementChangeRecordPath = "docs/normative/09_CONCORD_REQUIREMENT_CHANGE_RECORD_0.2.md"
$script:Phase10ReportPath = "docs/conformance/PHASE_10_REPORT.md"
$script:Phase11ReportPath = "docs/conformance/PHASE_11_REPORT.md"
$script:Phase12ReportPath = "docs/conformance/PHASE_12_REPORT.md"
$script:Phase13ReportPath = "docs/conformance/PHASE_13_REPORT.md"
$script:ControllerPath = "docs/controller/PROJECT_CONTROL.toml"
$script:Phase10Source = "9317a94cf020d81dbd216ed2a7dc999550586898"
$script:Phase10EvidenceState = "5b157225b387387664a202d4cbfe5f85ba8c5289"
$script:Phase10HistoricalBaseline = "6f7c3b69ec7f86a3eef4a9b70d94cf1ce449e52a"
$script:Phase10EvidenceBytes = 1081762
$script:Phase10EvidenceHash = "aadd64241a2dc42bb19dd01643915724df1a55390debbe52f7d7becbdc71e091"
$script:Phase10EvidenceRows = 1531
$script:Phase11HistoricalBaseline = "b66d2914609aec7b767481e09dedd5544b3d9130"
$script:Phase11EvidenceBytes = 1099185
$script:Phase11EvidenceHash = "90d31f417690082af865850d6e1ce25331bca16c76f37c9338382b71df85a91f"
$script:Phase11EvidenceRows = 1553
$script:Phase11ReportBytes = 25109
$script:Phase11ReportHash = "1de403491162bf39f36c530636f7b1dd03370ee45832c0dd2c7b9bc55d2b67fa"
$script:Phase12PreviousSource = "8307d0744363ea7ec8a98d5f5fe68c4429b71c31"
$script:Phase13PreviousSource = "684789b097b0cf225e880142874ae7eb0136286c"
$script:Phase13ProspectiveCorrectionBaseline = "44799306fbf1bd039b73c5e57903aed30e5e7a0d"
$script:Phase13BlockedSource = "d0891ac8de5e44c5da7d8d1202c3a9e4cfa4b304"
$script:Phase13SourceCorrectionPaths = [string[]]@(
    "scripts/audit-release-registry.ps1"
)
$script:Phase13PostSourcePaths = [string[]]@(
    "README.md",
    "docs/README.md",
    "docs/conformance/CONFORMANCE_EVIDENCE_0.1.csv",
    "docs/conformance/REQUIREMENT_COVERAGE_PLAN_0.1.csv",
    "docs/conformance/RELEASE_VALIDATION_REGISTRY_0.1.csv",
    "docs/conformance/PHASE_13_REPORT.md",
    "docs/planning/CONCORD_IMPLEMENTATION_PROGRAM_0.2.md",
    "docs/controller/PROJECT_CONTROL.toml"
)
$script:Phase13InheritedEvidenceSources = [string[]]@(
    "d0891ac8de5e44c5da7d8d1202c3a9e4cfa4b304",
    "47bee70ffea62e669c65f3ec8970730fab7b802d",
    "684789b097b0cf225e880142874ae7eb0136286c",
    "15b61d57c668fead307eafc7a276019760ae46f7",
    "dfc15d16aad6157c535f6e398bdf8317a79c71c7",
    "8cd5a60d1c64595b9235e58f7b0664d587e8cf82",
    "3ff65c60dc4f360663f13d0e122fa102f6d21999",
    "40cc4e7ff8869440248fbfea00f4b6c4fc0e6968"
)
$script:Phase13HistoricalBaseline = "0ef7d3f619155b251509da1d6112620b14f5c7b7"
$script:Phase14Baseline = "59e888a352b6bb612972066c07596b5ac28ad36b"
$script:Phase14HistoricalReleaseSource = "9317a94cf020d81dbd216ed2a7dc999550586898"
$script:Phase14HistoricalEvidenceRows = 1726
$script:Phase14HistoricalEvidenceBytes = 1387206
$script:Phase14HistoricalEvidenceHash = "649ee93b6be1c7df38b5f277c37113be9020a1a4d1ac669d99c451e513420056"
$script:Phase14HistoricalValidationRows = 237
$script:Phase14HistoricalValidationHash = "533fb9a52fead330e48383a71bbafa1fe8f45b7d6f70340ce00e6b9f62f0cae0"
$script:Phase14ClaimCount = 979
$script:Phase14ReportPath = "docs/conformance/PHASE_14_REPORT.md"
$script:Phase14EvidenceToken = "EVIDENCE_COMMIT_PENDING"
$script:Phase14AnchorToken = "ANCHOR_COMMIT_PENDING"
$script:Phase12EvidenceBytes = 1344331
$script:Phase12EvidenceHash = "8300c71829e3ed5f4049d74832bea64a3ed3c46de7b731555de4567a05334b05"
$script:Phase12EvidenceRows = 1680
$script:ActiveRequirementCount = 581
$script:Phase11SupersededRequirementIds = [string[]]@(
    "DSL-PAGE-004", "DSL-PAGE-005", "DSL-PAGE-006",
    "API-PAGE-004", "API-PAGE-005", "API-PAGE-006",
    "EXEC-PAGE-008", "EXEC-PAGE-014"
)
$script:Phase12SupersededRequirementIds = [string[]]@(
    "DSL-RESP-002", "SEC-RESP-004", "SEC-RESP-005"
)
$script:Phase13SupersededRequirementIds = [string[]]@(
    "DSL-RATE-003", "DSL-RATE-004", "DSL-RATE-005", "DSL-RATE-006", "DSL-RATE-007"
)
$script:SupersededRequirementIds = [string[]]@(
    "DSL-PAGE-004", "DSL-PAGE-005", "DSL-PAGE-006", "API-PAGE-004", "API-PAGE-005", "API-PAGE-006",
    "EXEC-PAGE-008", "EXEC-PAGE-014", "DSL-RESP-002", "SEC-RESP-004", "SEC-RESP-005",
    "DSL-RATE-003", "DSL-RATE-004", "DSL-RATE-005", "DSL-RATE-006", "DSL-RATE-007"
)
$script:Phase13RetainedRequirementIds = [string[]]@(
    "PROD-GUAR-010", "PROD-STATE-001", "DSL-RATE-001", "DSL-RATE-002", "API-RATE-004",
    "GEN-BUILD-004", "GEN-DOC-001", "GEN-DOC-002", "SEC-RATE-001", "SEC-RATE-002",
    "BOUND-RATE-005", "EXEC-RATE-001", "EXEC-RATE-002", "EXEC-RATE-003", "FEAT-GOV-001",
    "FEAT-GOV-002", "CONF-COMPILE-001", "CONF-COMPILE-002", "CONF-COMPILE-003",
    "CONF-DOWNSTREAM-002", "CONF-SEC-001", "CONF-MSRV-001", "CONF-DOC-001"
)
$script:Phase13RequirementIds = [string[]]@(
    "PROD-MODEL-009", "PROD-GUAR-015", "PROD-NONGOAL-003", "DSL-RATE-008", "DSL-RATE-009",
    "DSL-RATE-010", "DSL-RATE-011", "DSL-RATE-012", "API-RATE-005", "GEN-RATE-001",
    "GEN-RATE-002", "GEN-RATE-003", "GEN-RATE-004", "GEN-RATE-005", "GEN-RATE-006",
    "SEC-RATE-003", "EXEC-RATE-004", "EXEC-RATE-005", "COMPAT-MACRO-006", "CONF-RATE-001",
    "CONF-RATE-002", "CONF-RATE-003", "CONF-RATE-004"
)
$script:Phase11RequirementIds = [string[]]@(
    "DSL-PAGE-002", "API-PAGE-002", "SEC-PAGE-001", "SEC-PAGE-002",
    "EXEC-PAGE-001", "EXEC-PAGE-005", "EXEC-PAGE-007",
    "DSL-PAGE-007", "DSL-PAGE-008", "DSL-PAGE-009", "DSL-PAGE-010",
    "API-PAGE-007", "API-PAGE-008", "API-PAGE-009", "API-PAGE-010",
    "GEN-PAGE-004", "EXEC-PAGE-015", "EXEC-PAGE-016", "EXEC-PAGE-017",
    "SEC-PAGE-003", "COMPAT-MACRO-004", "CONF-PAGE-002"
)
$script:Phase12RequirementIds = [string[]]@(
    "PROD-CORE-002", "PROD-CORE-003", "PROD-GUAR-002", "PROD-GUAR-004",
    "PROD-GUAR-008", "PROD-GUAR-009", "PROD-GUAR-010", "DSL-BODY-001",
    "DSL-BODY-002", "DSL-END-001", "DSL-RESP-001", "DSL-FEAT-001",
    "DSL-FEAT-002", "API-CODEC-001",
    "API-CODEC-002", "API-CODEC-003", "API-CODEC-004", "API-BODY-001",
    "API-BODY-002", "API-BODY-003", "API-RESP-001", "API-PREP-001",
    "API-PREP-003", "GEN-END-001", "GEN-END-002", "GEN-TERM-001",
    "GEN-TERM-002", "GEN-PAGE-003", "GEN-PAGE-004", "SEC-BODY-001",
    "SEC-BODY-002", "SEC-RESP-003", "SEC-RESP-006", "SEC-RESP-007",
    "SEC-RESP-008", "BOUND-BODY-001", "BOUND-BODY-002", "BOUND-BODY-003",
    "BOUND-BODY-006", "BOUND-BODY-007", "BOUND-BODY-011", "EXEC-BODYMETA-001",
    "EXEC-BODY-001", "EXEC-BODY-002", "EXEC-BODY-003", "EXEC-BODY-004",
    "EXEC-BODY-005", "EXEC-BUFFER-001", "EXEC-BUFFER-002", "EXEC-BUFFER-003",
    "EXEC-NOCONTENT-001", "EXEC-STREAM-001", "EXEC-STREAM-002", "EXEC-STREAM-003",
    "EXEC-DEADLINE-001", "EXEC-DEADLINE-005", "EXEC-COMPLETE-003", "EXEC-RAW-001", "FEAT-JSON-001",
    "FEAT-MULTIPART-001", "FEAT-COMPRESS-001", "COMPAT-MACRO-001", "COMPAT-SURF-001",
    "COMPAT-BUILD-001", "CONF-COMPILE-001", "CONF-COMPILE-002", "CONF-COMPILE-003",
    "CONF-DOWNSTREAM-001", "CONF-DOWNSTREAM-002", "CONF-SEC-001", "CONF-MSRV-001",
    "CONF-DOC-001", "PROD-MODEL-008", "PROD-GUAR-014", "DSL-BODY-003",
    "DSL-RESP-003", "DSL-RESP-004", "DSL-FEAT-004", "API-BODYSPEC-001",
    "API-BODYSPEC-002", "API-BODYSPEC-003", "API-BODYSPEC-004", "API-RESPSPEC-001",
    "API-RESPSPEC-002", "API-RESPSPEC-003", "API-RESPSPEC-004", "API-RESP-002",
    "API-STREAM-001", "API-PAGE-011", "API-RECORD-001", "API-RECORD-002",
    "API-RECORD-003", "API-RECORD-004", "API-RECORD-005", "API-RECORD-006",
    "API-PREP-004", "GEN-BODY-001", "GEN-RESP-001", "GEN-RESP-002",
    "GEN-PAGE-005", "GEN-PREP-001", "SEC-BODY-003", "SEC-RESP-009",
    "SEC-STREAM-001", "SEC-RECORD-001", "SEC-RECORD-002", "BOUND-RECORD-001",
    "BOUND-RECORD-002", "BOUND-RECORD-003", "EXEC-BODY-006", "EXEC-BODY-007",
    "EXEC-RESP-001", "EXEC-RESP-002", "EXEC-STREAM-004", "EXEC-RECORD-001",
    "EXEC-RECORD-002", "EXEC-RECORD-003", "EXEC-RECORD-004", "EXEC-RECORD-005",
    "FEAT-RECORDS-001", "FEAT-CSV-001", "FEAT-NDJSON-001", "COMPAT-MACRO-005",
    "CONF-BODY-001", "CONF-RESP-001", "CONF-RECORD-001", "CONF-RECORD-002"
)

$script:NormativePaths = @(
    "docs/normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md",
    "docs/normative/01_CONCORD_PRODUCT_CONTRACT_0.1.md",
    "docs/normative/02_CONCORD_DSL_SPEC_0.1.md",
    "docs/normative/03_CONCORD_PUBLIC_API_SPEC_0.1.md",
    "docs/normative/04_CONCORD_GENERATED_API_SPEC_0.1.md",
    "docs/normative/05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md",
    "docs/normative/06_CONCORD_EXECUTION_SPEC_0.1.md",
    "docs/normative/07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md",
    "docs/normative/08_CONCORD_CONFORMANCE_PLAN_0.1.md"
)
$script:IndexedNormativePaths = @(
    $script:NormativePaths
    "docs/normative/09_CONCORD_REQUIREMENT_CHANGE_RECORD_0.2.md"
    "docs/normative/10_CONCORD_REQUIREMENT_CHANGE_RECORD_0.3.md"
    "docs/normative/11_CONCORD_REQUIREMENT_CHANGE_RECORD_0.4.md"
)
$script:ArchitecturePaths = @(
    "docs/architecture/CONCORD_CORE_ARCHITECTURE_0.1.md",
    "docs/architecture/CONCORD_MACRO_ARCHITECTURE_0.1.md",
    "docs/architecture/CONCORD_AUTH_STATE_MACHINE_0.1.md"
)
$script:AllowedLabels = [System.Collections.Generic.HashSet[string]]::new(
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

$root = (& git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($root)) {
    throw "unable to resolve the Git worktree root"
}
Set-Location -LiteralPath $root

function Assert-Condition {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function Assert-ExactCommit {
    param([string]$Commit, [string]$Name)
    Assert-Condition ($Commit -match '^[0-9a-f]{40}$') "$Name requires an exact 40-character commit"
    & git cat-file -e "$Commit`^{commit}" 2>$null
    Assert-Condition ($LASTEXITCODE -eq 0) "$Name commit does not exist: $Commit"
}

function Assert-Ancestor {
    param([string]$Ancestor, [string]$Descendant, [string]$Description)
    & git merge-base --is-ancestor $Ancestor $Descendant 2>$null
    Assert-Condition ($LASTEXITCODE -eq 0) "$Description is not ancestral: $Ancestor -> $Descendant"
}

function Assert-Phase13CorrectionBaselineInput {
    param(
        [string]$Baseline,
        [string]$Source,
        [scriptblock]$AncestorCheck
    )
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($Baseline)) "Phase 13 correction requires CorrectionBaseline"
    Assert-Condition ($Baseline -eq $script:Phase13ProspectiveCorrectionBaseline) "unexpected Phase 13 correction baseline: $Baseline"
    Assert-Condition ($Baseline -ne $Source) "Phase 13 correction baseline must precede SourceCommit"
    Assert-Condition (& $AncestorCheck $Baseline $Source) "Phase 13 correction baseline is not ancestral to SourceCommit: $Baseline -> $Source"
}

function Assert-Phase13SourceRangePaths {
    param([string[]]$Paths)
    $actual = @($Paths | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    Assert-Condition ($actual.Count -eq $script:Phase13SourceCorrectionPaths.Count) "prospective Phase 13 source range has an unexpected path count: $($actual.Count)"
    foreach ($path in $actual) {
        Assert-Condition ($script:Phase13SourceCorrectionPaths -contains $path) "prospective Phase 13 source range contains a non-executable or unapproved path: $path"
    }
    foreach ($path in $script:Phase13SourceCorrectionPaths) {
        Assert-Condition ($actual -contains $path) "prospective Phase 13 source range omits the required audit correction: $path"
    }
}

function Assert-Phase13PostSourcePaths {
    param([string[]]$Paths)
    foreach ($path in @($Paths | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })) {
        Assert-Condition ($script:Phase13PostSourcePaths -contains $path) "post-source executable or unapproved state change: $path"
    }
}

function Assert-Phase13ProspectiveSourceBoundary {
    param([string]$Baseline, [string]$Source)
    Assert-ExactCommit $Baseline "CorrectionBaseline"
    $gitAncestor = {
        param($left, $right)
        & git merge-base --is-ancestor $left $right 2>$null
        return $LASTEXITCODE -eq 0
    }
    Assert-Phase13CorrectionBaselineInput $Baseline $Source $gitAncestor
    $sourcePaths = @(& git diff --name-only "$Baseline..$Source")
    Assert-Condition ($LASTEXITCODE -eq 0) "prospective Phase 13 source-range audit failed"
    Assert-Phase13SourceRangePaths $sourcePaths
}

function Get-GitBlobText {
    param([string]$Commit, [string]$Path)
    $lines = @(& git show "$Commit`:$Path")
    Assert-Condition ($LASTEXITCODE -eq 0) "unable to read $Path at $Commit"
    return ($lines -join "`n")
}

function Get-GitBlobBytes {
    param([string]$Commit, [string]$Path)
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = "git"
    $start.WorkingDirectory = $root
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    if ($null -ne $start.PSObject.Properties["ArgumentList"]) {
        [void]$start.ArgumentList.Add("cat-file")
        [void]$start.ArgumentList.Add("blob")
        [void]$start.ArgumentList.Add("$Commit`:$Path")
    } else {
        Assert-Condition ($Path -notmatch '[\s"]') "legacy process invocation cannot quote path $Path"
        $start.Arguments = "cat-file blob $Commit`:$Path"
    }
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    Assert-Condition $process.Start() "unable to start git cat-file"
    $memory = [System.IO.MemoryStream]::new()
    $process.StandardOutput.BaseStream.CopyTo($memory)
    $errorText = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    Assert-Condition ($process.ExitCode -eq 0) "unable to read bytes for $Path at $Commit`: $errorText"
    $bytes = $memory.ToArray()
    $memory.Dispose()
    $process.Dispose()
    return ,$bytes
}

function Get-Sha256Hex {
    param([byte[]]$Bytes)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        $hash = $sha256.ComputeHash($Bytes)
        return (($hash | ForEach-Object { $_.ToString("x2") }) -join '')
    } finally {
        $sha256.Dispose()
    }
}

function Get-TextSha256 {
    param([string]$Text)
    return Get-Sha256Hex ([Text.Encoding]::UTF8.GetBytes($Text))
}

function Normalize-Prose {
    param([string]$Text)
    return ([regex]::Replace($Text.Trim(), '\s+', ' '))
}

function Import-GitCsv {
    param([string]$Commit, [string]$Path)
    $text = Get-GitBlobText $Commit $Path
    return @($text | ConvertFrom-Csv)
}

function Get-NormativeDocuments {
    param([string]$Commit)
    $documents = @{}
    foreach ($path in $script:NormativePaths) {
        $documents[$path] = Get-GitBlobText $Commit $path
    }
    return $documents
}

function Get-NormativeDefinitions {
    param([hashtable]$Documents)
    $definitions = [System.Collections.Generic.List[object]]::new()
    foreach ($path in $script:NormativePaths) {
        Assert-Condition $Documents.ContainsKey($path) "missing normative document $path"
        $lines = @($Documents[$path] -split "`n", 0, "SimpleMatch")
        for ($index = 0; $index -lt $lines.Count; $index++) {
            $match = [regex]::Match(
                $lines[$index],
                '^\*\*(?<id>[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+-[0-9]{3})\s+\S+\s+(?<title>.+?)\.\*\*(?:\s*(?<rest>.*))?$'
            )
            if (-not $match.Success) {
                continue
            }
            $bodyLines = [System.Collections.Generic.List[string]]::new()
            if (-not [string]::IsNullOrWhiteSpace($match.Groups["rest"].Value)) {
                $bodyLines.Add($match.Groups["rest"].Value)
            }
            $end = $index + 1
            while ($end -lt $lines.Count) {
                if ($lines[$end] -match '^\*\*[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+-[0-9]{3}\s+' -or
                    $lines[$end] -match '^#{1,6}\s') {
                    break
                }
                $bodyLines.Add($lines[$end])
                $end++
            }
            while ($bodyLines.Count -gt 0 -and [string]::IsNullOrWhiteSpace($bodyLines[$bodyLines.Count - 1])) {
                $bodyLines.RemoveAt($bodyLines.Count - 1)
            }
            $paragraph = Normalize-Prose ($bodyLines -join "`n")
            Assert-Condition (-not [string]::IsNullOrWhiteSpace($paragraph)) "empty normative definition $($match.Groups['id'].Value)"
            $definitions.Add([pscustomobject]@{
                requirement_id = $match.Groups["id"].Value
                normative_document = $path
                normative_title = $match.Groups["title"].Value
                normative_paragraph = $paragraph
                normative_paragraph_digest = Get-TextSha256 $paragraph
                start_line = $index + 1
                end_line = $end
                body_lines = [string[]]$bodyLines.ToArray()
            })
            $index = $end - 1
        }
    }
    return @($definitions)
}

function Get-ClaimUnits {
    param([object]$Definition)
    $units = [System.Collections.Generic.List[object]]::new()
    $lines = [string[]]$Definition.body_lines
    $index = 0
    while ($index -lt $lines.Count) {
        if ([string]::IsNullOrWhiteSpace($lines[$index])) {
            $index++
            continue
        }
        if ($lines[$index] -match '^```') {
            $code = [System.Collections.Generic.List[string]]::new()
            $index++
            while ($index -lt $lines.Count -and $lines[$index] -notmatch '^```') {
                if (-not [string]::IsNullOrWhiteSpace($lines[$index])) {
                    $code.Add($lines[$index].Trim())
                }
                $index++
            }
            if ($index -lt $lines.Count) {
                $index++
            }
            if ($code.Count -gt 0) {
                $units.Add([pscustomobject]@{ kind = "code"; text = Normalize-Prose ($code -join " ") })
            }
            continue
        }
        if ($lines[$index] -match '^\s*(?:-|[0-9]+\.)\s+(?<item>.*)$') {
            $item = [System.Collections.Generic.List[string]]::new()
            $item.Add($Matches["item"])
            $index++
            while ($index -lt $lines.Count -and
                -not [string]::IsNullOrWhiteSpace($lines[$index]) -and
                $lines[$index] -notmatch '^\s*(?:-|[0-9]+\.)\s+' -and
                $lines[$index] -notmatch '^```') {
                $item.Add($lines[$index].Trim())
                $index++
            }
            $units.Add([pscustomobject]@{ kind = "enumerated"; text = Normalize-Prose ($item -join " ") })
            continue
        }

        $paragraphLines = [System.Collections.Generic.List[string]]::new()
        while ($index -lt $lines.Count -and
            -not [string]::IsNullOrWhiteSpace($lines[$index]) -and
            $lines[$index] -notmatch '^\s*(?:-|[0-9]+\.)\s+' -and
            $lines[$index] -notmatch '^```') {
            $paragraphLines.Add($lines[$index].Trim())
            $index++
        }
        $paragraph = Normalize-Prose ($paragraphLines -join " ")
        foreach ($sentence in [regex]::Split($paragraph, '(?<=[.!?])\s+(?=[A-Z`])')) {
            $sentence = $sentence.Trim()
            if ([string]::IsNullOrWhiteSpace($sentence) -or $sentence.EndsWith(':')) {
                continue
            }
            $units.Add([pscustomobject]@{ kind = "sentence"; text = $sentence })
        }
    }
    return @($units)
}

function Get-ExpectedClaims {
    param([object[]]$Definitions)
    $claims = [System.Collections.Generic.List[object]]::new()
    foreach ($definition in $Definitions) {
        $ordinal = 0
        foreach ($unit in Get-ClaimUnits $definition) {
            $masked = [regex]::Replace($unit.text, '`[^`]*`', '')
            $modalMatches = @([regex]::Matches($masked, '\b(?:MUST NOT|MUST|SHOULD NOT|SHOULD|MAY)\b'))
            if ($definition.requirement_id -eq "DOC-LINT-001") {
                $modalMatches = @([pscustomobject]@{ Value = "MUST" })
            }
            $copies = [Math]::Max(1, $modalMatches.Count)
            for ($modalIndex = 0; $modalIndex -lt $copies; $modalIndex++) {
                $ordinal++
                if ($copies -gt 1) {
                    $atom = "Operative clause $($modalIndex + 1)/$copies ($($modalMatches[$modalIndex].Value)): $($unit.text)"
                } elseif ($unit.kind -eq "enumerated") {
                    $atom = "Enumerated clause: $($unit.text)"
                } elseif ($unit.kind -eq "code") {
                    $atom = "Canonical code clause: $($unit.text)"
                } else {
                    $atom = "Normative statement: $($unit.text)"
                }
                $claims.Add([pscustomobject]@{
                    claim_id = ("{0}-C{1:D3}" -f $definition.requirement_id, $ordinal)
                    requirement_id = $definition.requirement_id
                    claim_atom = $atom
                })
            }
        }
        Assert-Condition ($ordinal -gt 0) "normative requirement has no claim atom: $($definition.requirement_id)"
    }
    return @($claims)
}

function Assert-DurablePath {
    param([string]$Path, [string]$EvidenceId)
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($Path)) "empty exact path in $EvidenceId"
    Assert-Condition ($Path -eq $Path.Trim()) "untrimmed exact path in $EvidenceId`: '$Path'"
    Assert-Condition ($Path -notmatch '\\') "backslash path is not repository-stable in $EvidenceId`: $Path"
    Assert-Condition ($Path -notmatch '^(?:[A-Za-z]:|/|\.\.?/)' -and $Path -notmatch '/\.\.?/') "unsafe path in $EvidenceId`: $Path"
    Assert-Condition ($Path -notmatch '^(?:git_diff|target|\.git)(?:/|$)') "ignored or non-durable path in $EvidenceId`: $Path"
}

function Assert-KnownRequirement {
    param([string]$RequirementId, [System.Collections.Generic.HashSet[string]]$KnownIds, [string]$Context)
    Assert-Condition $KnownIds.Contains($RequirementId) "$Context has unknown requirement $RequirementId"
}

function Assert-RequirementMetadata {
    param([object]$Row, [object]$Definition)
    Assert-Condition ($Row.normative_document -eq $Definition.normative_document) "normative owner mismatch for $($Row.requirement_id)"
    Assert-Condition ($Row.normative_title -eq $Definition.normative_title) "normative title mismatch for $($Row.requirement_id)"
    Assert-Condition ($Row.normative_paragraph_digest -eq $Definition.normative_paragraph_digest) "normative paragraph digest mismatch for $($Row.requirement_id)"
}

function Assert-NoWaiver {
    param([string]$Result, [string]$Waiver, [string]$Context)
    Assert-Condition ($Result -ne "waived" -and [string]::IsNullOrWhiteSpace($Waiver)) "unapproved waiver in $Context"
}

function Assert-CompleteClaim {
    param([object]$Claim)
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($Claim.claim_atom)) "incomplete claim atom $($Claim.claim_id)"
    Assert-Condition ($Claim.result -eq "passing") "claim did not pass $($Claim.claim_id)"
    Assert-Condition ([string]::IsNullOrWhiteSpace($Claim.later_owner)) "unresolved claim atom $($Claim.claim_id)"
}

function Assert-CommitClass {
    param(
        [string]$VerifiedCommit,
        [string]$CommitClass,
        [string]$Source,
        [string]$Evidence,
        [string]$Historical,
        [scriptblock]$IsAncestor,
        [string]$EvidenceId
    )
    switch ($CommitClass) {
        "immutable_source" {
            Assert-Condition ($VerifiedCommit -eq $Source) "source evidence $EvidenceId uses $VerifiedCommit rather than $Source"
        }
        "evidence_state" {
            Assert-Condition ($VerifiedCommit -eq $Evidence) "evidence-state row $EvidenceId uses $VerifiedCommit rather than $Evidence"
        }
        "historical" {
            Assert-Condition (& $IsAncestor $VerifiedCommit $Historical) "historical row $EvidenceId is not ancestral to $Historical"
        }
        default { throw "unknown evidence commit class '$CommitClass' in $EvidenceId" }
    }
}

function Assert-EvidencePaths {
    param([object[]]$Rows, [string]$Descendant)
    $commits = @($Rows.verified_commit | Sort-Object -Unique)
    foreach ($commit in $commits) {
        Assert-ExactCommit $commit "evidence"
        Assert-Ancestor $commit $Descendant "evidence commit"
    }

    $queries = [System.Collections.Generic.List[string]]::new()
    $owners = [System.Collections.Generic.List[object]]::new()
    $uniquePaths = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Rows) {
        $parts = @($row.exact_path -split ';')
        Assert-Condition ($parts.Count -gt 0) "missing exact paths in $($row.evidence_id)"
        foreach ($part in $parts) {
            $path = $part.Trim()
            Assert-DurablePath $path $row.evidence_id
            [void]$uniquePaths.Add($path)
            $queries.Add("$($row.verified_commit):$path")
            $owners.Add([pscustomobject]@{ evidence_id = $row.evidence_id; path = $path })
        }
    }

    $ignored = @($uniquePaths | & git check-ignore --stdin)
    Assert-Condition ($LASTEXITCODE -in @(0, 1)) "git check-ignore failed"
    Assert-Condition ($ignored.Count -eq 0) "evidence cites ignored paths: $($ignored -join ', ')"

    $objects = @($queries | & git cat-file --batch-check='%(objectname) %(objecttype)')
    Assert-Condition ($LASTEXITCODE -eq 0 -and $objects.Count -eq $queries.Count) "commit-scoped path inspection failed"
    for ($index = 0; $index -lt $objects.Count; $index++) {
        if ($objects[$index] -match ' missing$') {
            throw "missing commit-scoped path in $($owners[$index].evidence_id): $($owners[$index].path)"
        }
        Assert-Condition ($objects[$index] -match ' (?:blob|tree)$') "unsupported Git object for $($owners[$index].path): $($objects[$index])"
    }
}

function Assert-HistoricalPrefix {
    param([string]$StateCommit)
    $historical = [byte[]](Get-GitBlobBytes $HistoricalBaseline $script:EvidencePath)
    $current = [byte[]](Get-GitBlobBytes $StateCommit $script:EvidencePath)
    Assert-Condition ($historical.Length -eq 1041408) "historical prefix byte count changed: $($historical.Length)"
    Assert-Condition ((Get-Sha256Hex $historical) -eq "a3f6878a4a15aa4d6c30616b1e26c8244568a32be04f1cb8ce4bb0a72dc983f1") "historical prefix hash changed"
    Assert-Condition ($current.Length -ge $historical.Length) "current evidence is shorter than the historical prefix"
    for ($index = 0; $index -lt $historical.Length; $index++) {
        if ($historical[$index] -ne $current[$index]) {
            throw "Phase 0-9 evidence prefix differs at byte $index"
        }
    }
    $historicalRows = @(Get-GitBlobText $HistoricalBaseline $script:EvidencePath | ConvertFrom-Csv)
    Assert-Condition ($historicalRows.Count -eq 1473) "historical prefix row count changed: $($historicalRows.Count)"
}

function Assert-BaseCatalogs {
    param([object[]]$Definitions, [object[]]$Coverage, [object[]]$Evidence)
    Assert-Condition ($Definitions.Count -eq $script:ActiveRequirementCount) "normative owner set contains $($Definitions.Count) definitions rather than $($script:ActiveRequirementCount)"
    Assert-Condition ($Coverage.Count -eq ($script:ActiveRequirementCount + $script:SupersededRequirementIds.Count)) "coverage registry has the wrong active/superseded row count: $($Coverage.Count)"
    $definitionIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($definition in $Definitions) {
        Assert-Condition $definitionIds.Add($definition.requirement_id) "duplicate normative owner $($definition.requirement_id)"
    }
    $coverageIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $activeCoverageIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $supersededCoverageIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Coverage) {
        Assert-Condition $coverageIds.Add($row.requirement_id) "duplicate coverage requirement $($row.requirement_id)"
        if ($row.status -eq "superseded") {
            Assert-Condition ($script:SupersededRequirementIds -contains $row.requirement_id) "unexpected superseded coverage row $($row.requirement_id)"
            Assert-Condition $supersededCoverageIds.Add($row.requirement_id) "duplicate superseded coverage row $($row.requirement_id)"
            $expectedOwner = if ($script:Phase13SupersededRequirementIds -contains $row.requirement_id) {
                "normative/11_CONCORD_REQUIREMENT_CHANGE_RECORD_0.4.md"
            } elseif ($script:Phase12SupersededRequirementIds -contains $row.requirement_id) {
                "normative/10_CONCORD_REQUIREMENT_CHANGE_RECORD_0.3.md"
            } else {
                "normative/09_CONCORD_REQUIREMENT_CHANGE_RECORD_0.2.md"
            }
            Assert-Condition ($row.document -eq $expectedOwner -or $row.document -match '^normative/0[1-8]_CONCORD_') "superseded coverage owner mismatch for $($row.requirement_id)"
        } else {
            Assert-Condition ($row.status -in @("planned", "implemented")) "invalid active coverage status $($row.status) for $($row.requirement_id)"
            Assert-KnownRequirement $row.requirement_id $definitionIds "active coverage row"
            [void]$activeCoverageIds.Add($row.requirement_id)
        }
    }
    foreach ($id in $definitionIds) {
        Assert-Condition $activeCoverageIds.Contains($id) "active normative ID is absent from coverage: $id"
    }
    foreach ($id in $script:SupersededRequirementIds) {
        Assert-Condition $supersededCoverageIds.Contains($id) "superseded ID is absent from lineage coverage: $id"
    }

    $knownEvidenceRequirements = [System.Collections.Generic.HashSet[string]]::new([string[]]$Definitions.requirement_id, [System.StringComparer]::Ordinal)
    foreach ($id in $script:SupersededRequirementIds) { [void]$knownEvidenceRequirements.Add($id) }
    $evidenceIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Evidence) {
        Assert-KnownRequirement $row.requirement_id $knownEvidenceRequirements "evidence row $($row.evidence_id)"
        Assert-Condition $evidenceIds.Add($row.evidence_id) "duplicate evidence ID $($row.evidence_id)"
        Assert-Condition $script:AllowedLabels.Contains($row.evidence_type) "unapproved evidence label $($row.evidence_type) in $($row.evidence_id)"
        Assert-Condition ($row.result -in @("passing", "waived")) "invalid result in $($row.evidence_id): $($row.result)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.exact_path)) "missing exact path in $($row.evidence_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.exact_test)) "missing exact command in $($row.evidence_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.verified_commit)) "missing verified commit in $($row.evidence_id)"
    }
}

function Assert-Phase10HistoricalCatalog {
    param([string]$StateCommit, [bool]$RequireExact)
    $historical = [byte[]](Get-GitBlobBytes $script:Phase10HistoricalBaseline $script:EvidencePath)
    Assert-Condition ($historical.Length -eq $script:Phase10EvidenceBytes) "Phase 0-10 evidence byte count changed at its owner: $($historical.Length)"
    Assert-Condition ((Get-Sha256Hex $historical) -eq $script:Phase10EvidenceHash) "Phase 0-10 evidence owner hash changed"
    $current = [byte[]](Get-GitBlobBytes $StateCommit $script:EvidencePath)
    Assert-Condition ($current.Length -ge $historical.Length) "current evidence is shorter than the Phase 0-10 catalog"
    for ($index = 0; $index -lt $historical.Length; $index++) {
        if ($historical[$index] -ne $current[$index]) {
            throw "Phase 0-10 evidence differs at byte $index"
        }
    }
    if ($RequireExact) {
        Assert-Condition ($current.Length -eq $historical.Length) "source identity contains post-Phase-10 evidence"
    }
    $historicalRows = @(Get-GitBlobText $script:Phase10HistoricalBaseline $script:EvidencePath | ConvertFrom-Csv)
    Assert-Condition ($historicalRows.Count -eq $script:Phase10EvidenceRows) "Phase 0-10 evidence row count changed: $($historicalRows.Count)"
}

function Assert-Phase10ReportPreserved {
    param([string]$StateCommit)
    $historical = [byte[]](Get-GitBlobBytes $script:Phase10HistoricalBaseline $script:Phase10ReportPath)
    $current = [byte[]](Get-GitBlobBytes $StateCommit $script:Phase10ReportPath)
    Assert-Condition ($historical.Length -eq $current.Length) "Phase 10 report byte count changed"
    Assert-Condition ((Get-Sha256Hex $historical) -eq (Get-Sha256Hex $current)) "Phase 10 report changed"
}

function Assert-Phase11HistoricalCatalog {
    param([string]$StateCommit, [bool]$RequireExact)
    $historical = [byte[]](Get-GitBlobBytes $script:Phase11HistoricalBaseline $script:EvidencePath)
    Assert-Condition ($historical.Length -eq $script:Phase11EvidenceBytes) "Phase 0-11 evidence byte count changed at its owner: $($historical.Length)"
    Assert-Condition ((Get-Sha256Hex $historical) -eq $script:Phase11EvidenceHash) "Phase 0-11 evidence owner hash changed"
    $current = [byte[]](Get-GitBlobBytes $StateCommit $script:EvidencePath)
    Assert-Condition ($current.Length -ge $historical.Length) "current evidence is shorter than the Phase 0-11 catalog"
    for ($index = 0; $index -lt $historical.Length; $index++) {
        if ($historical[$index] -ne $current[$index]) {
            throw "Phase 0-11 evidence differs at byte $index"
        }
    }
    if ($RequireExact) {
        Assert-Condition ($current.Length -eq $historical.Length) "source identity contains Phase 12 evidence"
    }
    $historicalRows = @(Get-GitBlobText $script:Phase11HistoricalBaseline $script:EvidencePath | ConvertFrom-Csv)
    Assert-Condition ($historicalRows.Count -eq $script:Phase11EvidenceRows) "Phase 0-11 evidence row count changed: $($historicalRows.Count)"
}

function Assert-Phase0Through11ReportsPreserved {
    param([string]$StateCommit)
    foreach ($phase in 0..11) {
        $path = "docs/conformance/PHASE_$($phase)_REPORT.md"
        $historical = [byte[]](Get-GitBlobBytes $script:Phase11HistoricalBaseline $path)
        $current = [byte[]](Get-GitBlobBytes $StateCommit $path)
        Assert-Condition ($historical.Length -eq $current.Length) "Phase $phase report byte count changed"
        Assert-Condition ((Get-Sha256Hex $historical) -eq (Get-Sha256Hex $current)) "Phase $phase report changed"
    }
    $phase11 = [byte[]](Get-GitBlobBytes $script:Phase11HistoricalBaseline $script:Phase11ReportPath)
    Assert-Condition ($phase11.Length -eq $script:Phase11ReportBytes) "Phase 11 report owner byte count changed"
    Assert-Condition ((Get-Sha256Hex $phase11) -eq $script:Phase11ReportHash) "Phase 11 report owner hash changed"
    $priorRecord = [byte[]](Get-GitBlobBytes $script:Phase11HistoricalBaseline $script:Phase11RequirementChangeRecordPath)
    $currentRecord = [byte[]](Get-GitBlobBytes $StateCommit $script:Phase11RequirementChangeRecordPath)
    Assert-Condition ($priorRecord.Length -eq $currentRecord.Length) "Phase 11 requirement-change record byte count changed"
    Assert-Condition ((Get-Sha256Hex $priorRecord) -eq (Get-Sha256Hex $currentRecord)) "Phase 11 requirement-change record changed"
}

function Assert-RequirementChangeRecord {
    param([string]$Commit)
    $record = Get-GitBlobText $Commit $script:RequirementChangeRecordPath
    foreach ($required in @(
        '**Status:** Normative Implementation Candidate',
        'authorizing Phase 13 directive',
        '| Implementation phase |',
        'Historical evidence remains valid only for the exact old paragraph version',
        'No Phase 0'
    )) {
        Assert-Condition $record.Contains($required) "requirement change record is missing: $required"
    }

    $historicalDocuments = Get-NormativeDocuments $script:Phase13HistoricalBaseline
    $historicalDefinitions = @(Get-NormativeDefinitions $historicalDocuments)
    $currentDocuments = Get-NormativeDocuments $Commit
    $currentDefinitions = @(Get-NormativeDefinitions $currentDocuments)
    $historicalById = @{}
    foreach ($definition in $historicalDefinitions) { $historicalById[$definition.requirement_id] = $definition }
    $currentById = @{}
    foreach ($definition in $currentDefinitions) { $currentById[$definition.requirement_id] = $definition }

    $retainedCount = 0
    $newCount = 0
    foreach ($id in $script:Phase13RetainedRequirementIds) {
        Assert-Condition $currentById.ContainsKey($id) "current Phase 13 retained requirement is absent: $id"
        Assert-Condition $historicalById.ContainsKey($id) "historical Phase 12 requirement is absent: $id"
        $current = $currentById[$id]
        $prior = $historicalById[$id]
        Assert-Condition ($record.Contains("| ``$id`` | $($prior.normative_title) |") -and $record.Contains("retained with explicit version change as ``$id``")) "retained lineage is incomplete for $id"
        $retainedCount++
    }
    foreach ($id in $script:Phase13RequirementIds) {
        Assert-Condition $currentById.ContainsKey($id) "current Phase 13 requirement is absent: $id"
        $current = $currentById[$id]
        $newRow = "| ``$id`` | $($current.normative_title) | ``$((Split-Path $current.normative_document -Leaf))`` |"
        Assert-Condition $record.Contains($newRow) "new Phase 13 lineage is incomplete for $id"
        $newCount++
    }
    Assert-Condition ($retainedCount -eq 23 -and $newCount -eq 23) "Phase 13 lineage counts changed: retained=$retainedCount new=$newCount"

    $replacementById = @{
        'DSL-RATE-003' = 'DSL-RATE-011'
        'DSL-RATE-004' = 'DSL-RATE-009'
        'DSL-RATE-005' = 'DSL-RATE-010'
        'DSL-RATE-006' = 'DSL-RATE-010'
        'DSL-RATE-007' = 'DSL-RATE-011'
    }
    foreach ($id in $script:Phase13SupersededRequirementIds) {
        $prior = $historicalById[$id]
        Assert-Condition ($record.Contains("| ``$id`` | $($prior.normative_title) |") -and $record.Contains("| superseded | ``$($replacementById[$id])`` |")) "Phase 13 superseded lineage is incomplete for $id"
    }

    $priorRecord = Get-GitBlobText $Commit $script:PriorRequirementChangeRecordPath
    foreach ($id in $script:Phase12SupersededRequirementIds) {
        Assert-Condition $priorRecord.Contains("``$id``") "prior lineage record does not resolve historical $id"
    }
}

function Assert-CandidateStatus {
    param([hashtable]$Documents, [string]$Commit)
    foreach ($path in $script:NormativePaths) {
        Assert-Condition $Documents[$path].Contains('**Status:** Normative Implementation Candidate') "normative document is not implementation-candidate: $path"
    }
    $record = Get-GitBlobText $Commit $script:RequirementChangeRecordPath
    Assert-Condition $record.Contains('**Status:** Normative Implementation Candidate') "requirement change record is not implementation-candidate"
    $program = Get-GitBlobText $Commit "docs/planning/CONCORD_IMPLEMENTATION_PROGRAM_0.2.md"
    foreach ($required in @(
        'Phase 11 | Uniform Typed Pagination State',
        'Phase 12 | Generic Body/Response Specifications and Integrated Streaming Records',
        'Phase 13 | Lexically Scoped Rate Limits',
        'Phase 14 | Product Completion, Conformance Closure, and Release Finalization',
        'generated contract `12 -> 13`',
        'Phase 12 approved implementation authority',
        'only Phase 14 may restore `Normative Release Final`'
    )) {
        Assert-Condition $program.Contains($required) "implementation program is missing: $required"
    }
}

function Assert-Phase12Authority {
    param([string]$Commit)
    $index = Get-GitBlobText $Commit 'docs/normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md'
    $decisions = Get-GitBlobText $Commit 'docs/decisions/CONCORD_DECISIONS.md'
    $coreArchitecture = Get-GitBlobText $Commit 'docs/architecture/CONCORD_CORE_ARCHITECTURE_0.1.md'
    $macroArchitecture = Get-GitBlobText $Commit 'docs/architecture/CONCORD_MACRO_ARCHITECTURE_0.1.md'
    foreach ($required in @(
        '10_CONCORD_REQUIREMENT_CHANGE_RECORD_0.3.md',
        'Normative Implementation Candidate'
    )) {
        Assert-Condition $index.Contains($required) "specification index is missing: $required"
    }
    foreach ($required in @(
        ('## ADR-045 ' + [char]0x2014 + ' Generic body and response specifications use sealed core acquisition modes'),
        '**Status:** Accepted; supersedes ADR-022',
        ('## ADR-022 ' + [char]0x2014 + ' Specialized protocols live outside core'),
        '**Status:** Superseded by ADR-045',
        'one stable `RequestBodySpec`', 'one stable `ResponseBodySpec`',
        'public-but-sealed core modes', '`ByteStream`', '`ApiResponse<T>`',
        'integrated in feature-gated `concord_core::records`',
        'neither a records crate, second executor'
    )) {
        Assert-Condition $decisions.Contains($required) "ADR authority is missing: $required"
    }
    foreach ($required in @(
        'type Body: RequestBodySpec;', 'type Response: ResponseBodySpec;',
        'Buffered  -> Bytes', 'Streaming -> ByteStream', 'Discard   -> ()',
        '`DecodedResponse<T>` is only a type alias',
        '`RecordBatches<D>` is a consumer adapter',
        'one execution engine serves generated and prepared endpoints'
    )) {
        Assert-Condition $coreArchitecture.Contains($required) "core architecture is missing: $required"
    }
    foreach ($required in @(
        'body: Option<syn::Type>', 'response: syn::Type',
        'B: RequestBodySpec', 'R: ResponseBodySpec',
        'R: PageResponseSpec', 'historical Phase 12 marker was `13`',
        'no body/response semantic enum or built-in-name branch'
    )) {
        Assert-Condition $macroArchitecture.Contains($required) "macro architecture is missing: $required"
    }
}

function Assert-Phase13Authority {
    param([string]$Commit)
    $index = Get-GitBlobText $Commit 'docs/normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md'
    $decisions = Get-GitBlobText $Commit 'docs/decisions/CONCORD_DECISIONS.md'
    $coreArchitecture = Get-GitBlobText $Commit 'docs/architecture/CONCORD_CORE_ARCHITECTURE_0.1.md'
    $macroArchitecture = Get-GitBlobText $Commit 'docs/architecture/CONCORD_MACRO_ARCHITECTURE_0.1.md'
    foreach ($required in @(
        '11_CONCORD_REQUIREMENT_CHANGE_RECORD_0.4.md', 'Normative Implementation Candidate'
    )) {
        Assert-Condition $index.Contains($required) "Phase 13 specification index is missing: $required"
    }
    foreach ($required in @(
        '## ADR-046', 'lexical ownership and global keys', 'normal DSL has no templates',
        'remote named attachments', '`by`', '`replace`', '`off`', 'formatted key parts',
        'advanced prepared/custom boundary'
    )) {
        Assert-Condition $decisions.Contains($required) "Phase 13 decision authority is missing: $required"
    }
    foreach ($required in @(
        'Normal generated code', 'RateLimitKey::global()', 'advanced prepared/custom boundary',
        'explicit `RateLimitPlan` containing bounded key parts'
    )) {
        Assert-Condition ($coreArchitecture.Contains($required) -or $macroArchitecture.Contains($required)) "Phase 13 architecture authority is missing: $required"
    }
    Assert-Condition $macroArchitecture.Contains('synchronized Phase 13 marker is `14`') "Phase 13 macro contract marker is missing"
}

function Assert-Phase12HistoricalCatalogPreserved {
    param([string]$StateCommit)
    $historical = [byte[]](Get-GitBlobBytes $script:Phase13HistoricalBaseline $script:EvidencePath)
    Assert-Condition ($historical.Length -eq $script:Phase12EvidenceBytes) "Phase 0-12 evidence owner byte count changed"
    Assert-Condition ((Get-Sha256Hex $historical) -eq $script:Phase12EvidenceHash) "Phase 0-12 evidence owner hash changed"
    $current = [byte[]](Get-GitBlobBytes $StateCommit $script:EvidencePath)
    Assert-Condition ($current.Length -ge $historical.Length) "current evidence is shorter than Phase 0-12 history"
    for ($index = 0; $index -lt $historical.Length; $index++) {
        if ($historical[$index] -ne $current[$index]) { throw "Phase 0-12 evidence differs at byte $index" }
    }
    $historicalRows = @(Get-GitBlobText $script:Phase13HistoricalBaseline $script:EvidencePath | ConvertFrom-Csv)
    Assert-Condition ($historicalRows.Count -eq $script:Phase12EvidenceRows) "Phase 0-12 evidence row count changed"
}

function Assert-Phase12ReportPreserved {
    param([string]$StateCommit)
    $historical = [byte[]](Get-GitBlobBytes $script:Phase13HistoricalBaseline $script:Phase12ReportPath)
    $current = [byte[]](Get-GitBlobBytes $StateCommit $script:Phase12ReportPath)
    Assert-Condition ($historical.Length -eq $current.Length) "Phase 12 report byte count changed"
    Assert-Condition ((Get-Sha256Hex $historical) -eq (Get-Sha256Hex $current)) "Phase 12 report changed"
}

function Assert-Phase13Evidence {
    param([object[]]$Evidence, [string]$Source)
    $rows = @($Evidence | Where-Object { $_.evidence_id -like 'P13-*' })
    Assert-Condition ($rows.Count -eq 46) "Phase 13 evidence row count is $($rows.Count) rather than 46"
    $ids = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $rows) {
        Assert-Condition $ids.Add($row.requirement_id) "duplicate Phase 13 evidence requirement $($row.requirement_id)"
        Assert-Condition ($row.verified_commit -eq $Source) "Phase 13 evidence has wrong source: $($row.evidence_id)"
        Assert-Condition ($row.result -eq 'passing' -and $row.configuration -match '^direct ') "Phase 13 evidence is not direct passing: $($row.evidence_id)"
        Assert-Condition ([string]::IsNullOrWhiteSpace($row.waiver)) "Phase 13 evidence has a waiver: $($row.evidence_id)"
    }
    foreach ($id in @($script:Phase13RetainedRequirementIds + $script:Phase13RequirementIds)) {
        Assert-Condition $ids.Contains($id) "Phase 13 direct evidence is absent for $id"
    }
}

function Assert-Phase13ValidationRegistry {
    param([object[]]$Rows, [string]$Source)
    $phaseRows = @($Rows | Where-Object { $_.validation_id -like 'P13-VAL-*' })
    Assert-Condition ($phaseRows.Count -ge 24) "Phase 13 validation registry row count is $($phaseRows.Count) rather than at least 24"
    $commands = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $phaseRows) {
        Assert-Condition $commands.Add($row.exact_command) "duplicate Phase 13 validation command $($row.exact_command)"
        Assert-Condition ($row.working_directory -eq 'F:\projects\concord_api') "wrong Phase 13 validation working directory $($row.validation_id)"
        Assert-Condition ($row.source_commit -eq $Source -and $row.exit_status -eq '0' -and $row.result -eq 'passing') "Phase 13 validation did not pass $($row.validation_id)"
        foreach ($field in @('tool_versions', 'feature_configuration', 'selected_tests', 'observed_at')) {
            Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.$field)) "Phase 13 validation omitted $field in $($row.validation_id)"
        }
    }
    foreach ($command in @(
        'just fmt-check', 'just check', 'just check-all', 'just check-no-default',
        'just clippy', 'just clippy-all', 'just test', 'just test-all',
        'just test-no-default', 'just doctest', 'just doctest-all', 'just doc',
        'just doc-all', 'just deny', 'just diff-check', 'just release',
        'cargo nextest run --profile ci -p concord_macros --test generated_rate_limiting --all-features -E "test(inherited_lexical_declarations_keep_one_identity_across_descendant_modules)"',
        'cargo nextest run --profile ci -p concord_macros --test generated_rate_limiting --all-features -E "test(declaration_identities_are_deterministic_and_collision_resistant)"',
        'cargo nextest run --profile ci -p concord_macros --test generated_rate_limiting --all-features -E "test(punctuation_distinct_lexical_paths_have_distinct_private_symbols)"',
        'cargo nextest run --profile ci -p concord_macros --test generated_rate_limiting --all-features -E "test(client_rule_quota_is_shared_across_direct_and_scoped_endpoints) | test(scope_rule_quota_is_shared_across_immediate_and_nested_descendants) | test(sibling_and_endpoint_local_rules_do_not_share_unrelated_buckets)"',
        'cargo nextest run --profile ci -p concord_core --test pagination_rate_limit --all-features -E "test(pagination_reuses_identical_lexical_plan_across_multiple_pages)"',
        'cargo nextest run --profile ci -p concord_core --test pagination_rate_limit --all-features -E "test(pagination_pages_share_the_same_lexical_governor_bucket)"',
        'cargo nextest run --profile ci -p concord_core --test pagination_rate_limit --all-features -E "test(pagination_cancellation_prevents_later_lexical_plan_acquisition)"',
        'cargo nextest run --profile ci -p concord_core --all-features --lib -E "test(/execute::auth_tests::paginated_tls_recovery_keeps_one_page_identity_index_and_completion$/)"',
        'cargo nextest run --profile ci -p concord_core --all-features --lib -E "test(/execute::tests::default_key_limit_accepts_1024_bytes_and_rejects_the_first_excess$/) | test(/rate_limit::governor::tests::configured_key_limit_revalidates_before_governor_state_insertion$/)"',
        'cargo nextest run --profile ci -p concord_core --all-features --lib -E "test(/execute::auth_tests::configured_key_limit_failure_cuts_off_every_protected_side_effect_and_redacts_key$/) | test(/execute::tests::prepared_plans_use_the_selected_governor_and_its_configured_key_limit$/)"',
        'cargo nextest run --profile ci -p concord_macros --test compile --all-features -E "test(generated_public_rustdoc_uses_stable_paths_without_private_integration_names)"',
        'cargo nextest run --profile ci -p concord_core --test public_rustdoc --all-features',
        'just _examples',
        'just _msrv',
        'cargo nextest run --profile ci -p concord_core --all-features --lib -E "test(/execute::tests::deadline_cancellation_of_governor_wait_consumes_no_quota$/) | test(/execute::auth_tests::real_tls_authentication_recovery_performs_two_visible_limiter_acquisitions$/) | test(/execute::auth_tests::paginated_tls_recovery_keeps_one_page_identity_index_and_completion$/) | test(/execute::tests::hidden_reqwest_retry_does_not_reenter_custom_limiter_or_lifecycle$/)"',
        'cargo nextest run --profile ci -p concord_core --test pagination_rate_limit --all-features -E "test(generated_pagination_uses_a_value_neutral_global_bucket)"',
        'cargo nextest run --offline --locked --target-dir target/cargo-fixtures/phase13-no-governor-direct --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-no-governor-rate-limits --test no_governor_rate_limits --no-tests fail --success-output final',
        'cargo tree --offline --locked --manifest-path concord_macros/tests/fixtures/Cargo.toml -e features -p concord-fixture-no-governor-rate-limits',
        'cargo nextest run --profile ci -p concord_macros --test compile --all-features -E "test(governor_disabled_generated_and_prepared_plans_fail_closed_or_require_deliberate_limiter)"',
        'cargo nextest run --profile ci -p concord_macros --test generated_prepared_endpoints --all-features -E "test(prepared_custom_limiter_inspects_ids_keys_costs_and_windows_in_call_order)"',
        "pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode Source -CorrectionBaseline $script:Phase13ProspectiveCorrectionBaseline -SourceCommit $Source"
    )) {
        Assert-Condition $commands.Contains($command) "required Phase 13 validation command was not recorded: $command"
    }
}

function Assert-Phase13EvidenceCommandsRecorded {
    param([object[]]$Evidence, [object[]]$Validation)
    $commands = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Validation | Where-Object { $_.validation_id -like 'P13-VAL-*' }) {
        [void]$commands.Add($row.exact_command)
    }
    foreach ($row in $Evidence | Where-Object { $_.evidence_id -like 'P13-*' }) {
        $proofs = @($row.exact_test -split '\s+;;\s+')
        Assert-Condition ($proofs.Count -gt 0) "Phase 13 evidence has no proof command: $($row.evidence_id)"
        foreach ($proof in $proofs) {
            Assert-Condition (-not [string]::IsNullOrWhiteSpace($proof)) "Phase 13 evidence has an empty proof command: $($row.evidence_id)"
            Assert-Condition ($proof -ne 'just release') "Phase 13 evidence uses only just release: $($row.evidence_id)"
            Assert-Condition $commands.Contains($proof) "Phase 13 evidence command lacks a passing validation record: $($row.evidence_id): $proof"
        }
    }
}

function Assert-Phase13CorrectionEvidenceAllocation {
    param([object[]]$Evidence)
    $byRequirement = @{}
    foreach ($row in $Evidence | Where-Object { $_.evidence_id -like 'P13-*' }) {
        $byRequirement[$row.requirement_id] = $row.exact_test
    }
    function RequireProof {
        param([string]$Requirement, [string[]]$Needles)
        Assert-Condition $byRequirement.ContainsKey($Requirement) "Phase 13 correction evidence omitted $Requirement"
        $proof = $byRequirement[$Requirement]
        foreach ($needle in $Needles) {
            Assert-Condition $proof.Contains($needle) "Phase 13 correction evidence omitted `$needle` for $Requirement"
        }
    }
    $fixtureCommand = 'cargo nextest run --offline --locked --target-dir target/cargo-fixtures/phase13-no-governor-direct --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-no-governor-rate-limits --test no_governor_rate_limits --no-tests fail --success-output final'
    $featureCommand = 'cargo tree --offline --locked --manifest-path concord_macros/tests/fixtures/Cargo.toml -e features -p concord-fixture-no-governor-rate-limits'
    $outerCommand = 'cargo nextest run --profile ci -p concord_macros --test compile --all-features -E "test(governor_disabled_generated_and_prepared_plans_fail_closed_or_require_deliberate_limiter)"'
    $planCommand = 'cargo nextest run --profile ci -p concord_macros --test generated_prepared_endpoints --all-features -E "test(prepared_custom_limiter_inspects_ids_keys_costs_and_windows_in_call_order)"'
    RequireProof 'FEAT-GOV-001' @($fixtureCommand, $outerCommand, $featureCommand, 'generated_default_limiter_selection_matches_the_resolved_feature_graph')
    RequireProof 'FEAT-GOV-002' @($fixtureCommand)
    RequireProof 'GEN-RATE-005' @($fixtureCommand)
    RequireProof 'API-RATE-004' @($planCommand, 'default_key_limit_accepts_1024_bytes_and_rejects_the_first_excess')
    RequireProof 'CONF-RATE-002' @(
        'inherited_lexical_declarations_keep_one_identity_across_descendant_modules',
        'declaration_identities_are_deterministic_and_collision_resistant',
        'punctuation_distinct_lexical_paths_have_distinct_private_symbols',
        'client_rule_quota_is_shared_across_direct_and_scoped_endpoints',
        'scope_rule_quota_is_shared_across_immediate_and_nested_descendants',
        'pagination_reuses_identical_lexical_plan_across_multiple_pages',
        'pagination_pages_share_the_same_lexical_governor_bucket',
        'generated_pagination_uses_a_value_neutral_global_bucket'
    )
    RequireProof 'CONF-DOC-001' @(
        'just _examples',
        'generated_public_rustdoc_uses_stable_paths_without_private_integration_names',
        'cargo nextest run --profile ci -p concord_core --test public_rustdoc --all-features'
    )
    RequireProof 'CONF-SEC-001' @(
        'request_debug_is_emitted_before_native_boundary_and_never_contains_auth',
        'generated_public_rustdoc_uses_stable_paths_without_private_integration_names',
        'cargo nextest run --profile ci -p concord_core --test public_rustdoc --all-features'
    )
}

function Assert-Phase13CorrectionExecutableInputs {
    param([string]$Commit)
    $fixtureManifest = Get-GitBlobText $Commit 'concord_macros/tests/fixtures/no_governor_rate_limits/Cargo.toml'
    $fixtureTest = Get-GitBlobText $Commit 'concord_macros/tests/fixtures/no_governor_rate_limits/tests/no_governor_rate_limits.rs'
    $compileHarness = Get-GitBlobText $Commit 'concord_macros/tests/compile.rs'
    Assert-Condition $fixtureManifest.Contains('concord_core = { path = "../../../../concord_core", default-features = false }') 'governor-disabled fixture does not disable concord_core default features'
    Assert-Condition (-not $fixtureManifest.Contains('rate-limit-governor')) 'governor-disabled fixture enables rate-limit-governor'
    Assert-Condition $fixtureTest.Contains('CONCORD_NO_GOVERNOR_RATE_LIMITS_OK') 'governor-disabled fixture lacks execution sentinel'
    Assert-Condition $fixtureTest.Contains('generated_clients_fail_closed_without_governor_unless_limiter_is_deliberate') 'governor-disabled generated proof is absent'
    Assert-Condition $compileHarness.Contains('governor_disabled_generated_and_prepared_plans_fail_closed_or_require_deliberate_limiter') 'governor-disabled outer harness selector is absent'
}

function Assert-Phase13RetainedAndConformanceEvidenceAllocation {
    param([object[]]$Evidence)
    $byRequirement = @{}
    foreach ($row in $Evidence | Where-Object { $_.evidence_id -like 'P13-*' }) {
        $byRequirement[$row.requirement_id] = $row
    }
    function RequireProof {
        param([string]$Requirement, [string[]]$Needles)
        Assert-Condition $byRequirement.ContainsKey($Requirement) "Phase 13 retained/conformance evidence omitted $Requirement"
        $proof = $byRequirement[$Requirement].exact_test
        foreach ($needle in $Needles) {
            Assert-Condition $proof.Contains($needle) "Phase 13 evidence omitted `$needle` for $Requirement"
        }
    }
    $fixture = 'no_governor_rate_limits --no-tests fail --success-output final'
    RequireProof 'PROD-GUAR-010' @(
        'authenticated_cleartext_fails_in_public_preparation_before_hook_body_or_provider',
        'https_failure_never_falls_back_to_plain_http',
        'metadata_contradictions_fail_after_hook_and_before_limiter_or_native_send',
        'encoding_failure_and_request_limit_prevent_native_execution_safely',
        'buffered_decoder_never_observes_known_or_first_excess_response_bytes',
        'unknown_stream_is_lazy_and_first_cumulative_excess_chunk_is_withheld',
        'cumulative_limit_drops_and_withholds_the_first_excess_chunk',
        'first_staging_excess_is_withheld_and_error_formatting_omits_bytes',
        $fixture,
        'immediate_two_state_and_long_cycles_stop_before_protected_page_work'
    )
    RequireProof 'PROD-STATE-001' @(
        'client_core_freezes_complete_runtime_selection_at_construction',
        'selected_governor_and_preparation_limit_form_one_runtime_snapshot',
        'request_creation_is_lazy_and_generated_client_clone_shares_runtime_configuration',
        'generated_handles_share_clone_state_keep_independent_builds_and_replace_atomically'
    )
    RequireProof 'DSL-RATE-002' @(
        'lexical_rate_limit_declarations_parse_in_order',
        'rate_limit_cost_defaults_and_equal_capacity_is_valid',
        'rate_limit_syntax_rejects_invalid_structure_values_dimensions_and_order',
        'rate_limit_diagnostics_are_source_only_and_focused'
    )
    RequireProof 'GEN-BUILD-004' @(
        'configured_observer_replaces_rather_than_composes_with_default',
        'generated_default_limiter_selection_matches_the_resolved_feature_graph',
        'governor_disabled_generated_and_prepared_plans_fail_closed_or_require_deliberate_limiter',
        'generated_handles_share_clone_state_keep_independent_builds_and_replace_atomically'
    )
    RequireProof 'EXEC-RATE-002' @(
        'multiple_rules_and_windows_wait_for_the_complete_maximum',
        'unavailable_rule_causes_no_partial_reservation_or_lock_across_wait',
        'cancellation_while_waiting_consumes_no_future_quota',
        'successful_reservation_is_not_refunded_after_later_cancellation',
        'boundary_helpers_are_checked'
    )
    RequireProof 'CONF-COMPILE-001' @(
        'complete_extension_fixture_compiles_through_public_namespaces_for_every_owned_feature_set',
        'downstream_pagination_runtime_executes_generated_construction_and_binding',
        'generated_public_rustdoc_uses_stable_paths_without_private_integration_names',
        'just _feature-matrix',
        'just _examples'
    )
    RequireProof 'CONF-COMPILE-002' @(
        'complete_extension_fixture_compiles_through_public_namespaces_for_every_owned_feature_set',
        'complete_extension_fixture_has_no_private_native_or_integration_escape',
        'just _feature-matrix'
    )
    RequireProof 'CONF-DOWNSTREAM-002' @(
        'downstream_credential_limiter_and_hooks_execute_through_public_runtime',
        'downstream_pagination_runtime_executes_generated_construction_and_binding',
        'downstream_request_and_all_sealed_response_modes_work_without_macro_changes',
        'downstream_decoder_streams_and_batches_through_generated_public_contracts',
        'complete_extension_fixture_has_no_private_native_or_integration_escape'
    )
    RequireProof 'CONF-RATE-004' @(
        'before_send_veto_prevents_native_execution_and_later_response_phases',
        'limiter_failure_prevents_request_debug_and_native_execution',
        'deadline_cancellation_of_governor_wait_consumes_no_quota',
        'multiple_rules_and_windows_wait_for_the_complete_maximum',
        'successful_reservation_is_not_refunded_after_later_cancellation',
        'real_tls_authentication_recovery_performs_two_visible_limiter_acquisitions',
        'hidden_reqwest_retry_does_not_reenter_custom_limiter_or_lifecycle',
        'raw_response_reuses_limiter_hooks_observation_and_success_completion',
        'quota_state_is_bounded_idle_pruned_and_eviction_allows_fresh_burst',
        'current_bucket_cooldown_is_exact_and_empty_plan_falls_back_to_host',
        'host_and_client_cooldowns_affect_only_their_normative_scopes',
        'cooldown_is_clamped_extended_never_shortened_and_zero_is_noop',
        'cooldown_state_capacity_uses_deterministic_eviction',
        'default_retry_after_cooldown_is_future_only_host_fallback_and_runtime_capped',
        'configured_observer_replaces_rather_than_composes_with_default',
        'poisoned_state_fails_future_acquisition_closed',
        'explicit_noop_authorizes_nonempty_generated_plan_without_dsl_observer',
        'governor_disabled_generated_and_prepared_plans_fail_closed_or_require_deliberate_limiter',
        'pagination_reuses_identical_lexical_plan_across_multiple_pages',
        'pagination_cancellation_prevents_later_lexical_plan_acquisition'
    )
}

function Assert-Phase13ClaimAudit {
    param([object[]]$Evidence, [object[]]$Validation, [string]$Source)
    $auditPath = Join-Path $root 'git_diff/phase-13-evidence-claim-audit.md'
    Assert-Condition (Test-Path -LiteralPath $auditPath) "Phase 13 claim audit is missing: $auditPath"
    $audit = Get-Content -LiteralPath $auditPath -Raw
    $sections = [regex]::Matches($audit, '(?m)^### P13-[^\r\n]+$')
    Assert-Condition ($sections.Count -eq 46) "Phase 13 claim audit contains $($sections.Count) sections rather than 46"
    Assert-Condition ([regex]::Matches($audit, [regex]::Escape($Source)).Count -ge 46) "Phase 13 claim audit is not source-matched to $Source"
    foreach ($stale in $script:Phase13InheritedEvidenceSources) {
        if ($stale -ne $Source) {
            Assert-Condition (-not $audit.Contains($stale)) "Phase 13 claim audit retains stale source identity: $stale"
        }
    }
    foreach ($row in $Evidence | Where-Object { $_.evidence_id -like 'P13-*' }) {
        Assert-Condition $audit.Contains($row.evidence_id) "Phase 13 claim audit omitted $($row.evidence_id)"
        Assert-Condition $audit.Contains($row.exact_test) "Phase 13 claim audit omitted the exact command allocation for $($row.evidence_id)"
        Assert-Condition ($row.verified_commit -eq $Source) "Phase 13 evidence source is stale for $($row.evidence_id)"
    }
    foreach ($line in ($audit -split "`r?`n")) {
        if ($line -match '^\s+-\s+`(cargo|just)\s+(.+)`\s*$') {
            $command = "$($Matches[1]) $($Matches[2])"
            Assert-Condition (@($Evidence | Where-Object { $_.evidence_id -like 'P13-*' -and $_.exact_test.Contains($command) }).Count -gt 0) "Phase 13 claim audit contains an unallocated command: $command"
        }
    }
    foreach ($row in $Validation | Where-Object { $_.validation_id -like 'P13-VAL-*' }) {
        Assert-Condition ($row.source_commit -eq $Source) "Phase 13 validation source is stale for $($row.validation_id)"
    }
}

function Assert-Phase13StateDocumentation {
    param([string]$Commit, [string]$Source, [string]$EvidenceState, [string]$Anchor)
    $report = Get-GitBlobText $Commit $script:Phase13ReportPath
    $controller = Get-GitBlobText $Commit $script:ControllerPath
    foreach ($required in @(
        'PHASE_13_COMPLETE_WITH_LIMITATIONS', $Source, $EvidenceState, $Anchor,
        $script:Phase13ProspectiveCorrectionBaseline, $script:Phase13BlockedSource,
        'generated contract transition exactly', 'Phase 14', '`NOT_STARTED`',
        '46 direct and 0 partial', '1,726 total rows', 'Normative Implementation Candidate'
    )) {
        Assert-Condition $report.Contains($required) "Phase 13 report is missing: $required"
    }
    foreach ($required in @(
        'latest_validated_phase = "13"', 'latest_verdict = "PHASE_13_COMPLETE_WITH_LIMITATIONS"',
        "source_commit = `"$Source`"", "evidence_commit = `"$EvidenceState`"",
        "anchor_commit = `"$Anchor`"", 'head_policy = "anchor-must-be-an-ancestor"',
        'normative_status = "Normative Implementation Candidate"',
        'id = "14"', 'title = "Product Completion, Conformance Closure, and Release Finalization"',
        'status = "NOT_STARTED"'
    )) {
        Assert-Condition $controller.Contains($required) "Phase 13 controller state is missing: $required"
    }
    Assert-Condition (-not $report.Contains('Implementation result: `PHASE_BLOCKED`')) "Phase 13 report retains a blocked implementation result"
    Assert-Condition (-not $controller.Contains('latest_verdict = "PHASE_BLOCKED"')) "Phase 13 controller retains a blocked verdict"
}

function Assert-SupersededHistoricalEvidence {
    param([object[]]$Evidence)
    foreach ($row in @($Evidence | Where-Object { $script:SupersededRequirementIds -contains $_.requirement_id })) {
        Assert-ExactCommit $row.verified_commit "superseded historical evidence"
        Assert-Ancestor $row.verified_commit $script:Phase10Source "superseded evidence/source lineage"
        Assert-Condition ($row.evidence_id -notlike 'P11-*') "Phase 11 evidence uses superseded requirement $($row.requirement_id)"
    }
}

function Assert-Phase11Evidence {
    param([object[]]$Evidence, [string]$Source)
    $rows = @($Evidence | Where-Object { $_.evidence_id -like 'P11-*' })
    Assert-Condition ($rows.Count -eq $script:Phase11RequirementIds.Count) "Phase 11 evidence row count is $($rows.Count) rather than $($script:Phase11RequirementIds.Count)"
    $ids = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $rows) {
        Assert-Condition $ids.Add($row.requirement_id) "duplicate Phase 11 evidence requirement $($row.requirement_id)"
        Assert-Condition ($row.verified_commit -eq $Source) "Phase 11 evidence $($row.evidence_id) does not name the tested source"
        Assert-Condition ($row.result -eq 'passing' -and $row.configuration -match '^direct ') "Phase 11 evidence is not direct passing: $($row.evidence_id)"
        Assert-Condition ([string]::IsNullOrWhiteSpace($row.waiver)) "Phase 11 evidence has a waiver: $($row.evidence_id)"
    }
    foreach ($id in $script:Phase11RequirementIds) {
        Assert-Condition $ids.Contains($id) "Phase 11 direct evidence is absent for $id"
    }
}

function Assert-InheritedPhase11SourceEvidence {
    param([object[]]$Evidence, [string]$Source)
    $rows = @($Evidence | Where-Object { $_.evidence_id -like 'P11-*' })
    Assert-Condition ($rows.Count -eq $script:Phase11RequirementIds.Count) "correction source inherited $($rows.Count) Phase 11 rows rather than $($script:Phase11RequirementIds.Count)"
    $ids = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $rows) {
        Assert-Condition $ids.Add($row.requirement_id) "duplicate inherited Phase 11 evidence requirement $($row.requirement_id)"
        Assert-ExactCommit $row.verified_commit "inherited Phase 11 evidence"
        Assert-Condition ($row.verified_commit -ne $Source) "correction source already contains evidence claiming its own unvalidated identity: $($row.evidence_id)"
        Assert-Ancestor $row.verified_commit $Source "inherited Phase 11 evidence/source lineage"
        Assert-Condition ($row.result -eq 'passing' -and $row.configuration -match '^direct ') "inherited Phase 11 evidence is not direct passing: $($row.evidence_id)"
        Assert-Condition ([string]::IsNullOrWhiteSpace($row.waiver)) "inherited Phase 11 evidence has a waiver: $($row.evidence_id)"
    }
    foreach ($id in $script:Phase11RequirementIds) {
        Assert-Condition $ids.Contains($id) "correction source did not inherit Phase 11 evidence for $id"
    }
}

function Assert-Phase11ValidationRegistry {
    param([object[]]$Rows, [string]$Source)
    $phaseRows = @($Rows | Where-Object { $_.validation_id -like 'P11-VAL-*' })
    Assert-Condition ($phaseRows.Count -ge 16) "Phase 11 validation registry is incomplete"
    $commands = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $phaseRows) {
        Assert-Condition $commands.Add($row.exact_command) "duplicate Phase 11 validation command $($row.exact_command)"
        Assert-Condition ($row.working_directory -eq 'F:\projects\concord_api') "wrong Phase 11 validation working directory $($row.validation_id)"
        Assert-Condition ($row.source_commit -eq $Source) "Phase 11 validation used wrong source $($row.validation_id)"
        Assert-Condition ($row.exit_status -eq '0' -and $row.result -eq 'passing') "Phase 11 validation did not pass $($row.validation_id)"
        foreach ($field in @('tool_versions', 'feature_configuration', 'selected_tests', 'observed_at')) {
            Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.$field)) "Phase 11 validation omitted $field in $($row.validation_id)"
        }
    }
    foreach ($command in @(
        'just fmt-check', 'just check', 'just check-all', 'just check-no-default',
        'just clippy', 'just clippy-all', 'just test', 'just test-all',
        'just test-no-default', 'just doctest', 'just doctest-all', 'just doc',
        'just doc-all', 'just deny', 'just diff-check', 'just release'
    )) {
        Assert-Condition $commands.Contains($command) "required Phase 11 validation command was not recorded: $command"
    }
}

function Assert-Phase11StateDocumentation {
    param([string]$Commit, [string]$Source, [string]$EvidenceState, [string]$Anchor)
    $report = Get-GitBlobText $Commit $script:Phase11ReportPath
    $controller = Get-GitBlobText $Commit $script:ControllerPath
    foreach ($required in @(
        'PHASE_11_COMPLETE_WITH_LIMITATIONS', $Source, $EvidenceState, $Anchor,
        'generated contract `11 -> 12`', 'Phase 12', '`NOT_STARTED`',
        'Normative Implementation Candidate', 'Phase 14'
    )) {
        Assert-Condition $report.Contains($required) "Phase 11 report is missing: $required"
    }
    foreach ($required in @(
        'latest_validated_phase = "11"', 'latest_verdict = "PHASE_11_COMPLETE_WITH_LIMITATIONS"',
        "source_commit = `"$Source`"", "evidence_commit = `"$EvidenceState`"",
        "anchor_commit = `"$Anchor`"", 'head_policy = "anchor-must-be-an-ancestor"',
        'normative_status = "Normative Implementation Candidate"',
        'id = "12"', 'title = "Generic Body/Response Specifications and Integrated Streaming Records"',
        'status = "NOT_STARTED"'
    )) {
        Assert-Condition $controller.Contains($required) "Phase 11 controller state is missing: $required"
    }
}

function Assert-Phase12Evidence {
    param([object[]]$Evidence, [string]$Source)
    $rows = @($Evidence | Where-Object { $_.evidence_id -like 'P12-*' })
    Assert-Condition ($rows.Count -eq $script:Phase12RequirementIds.Count) "Phase 12 evidence row count is $($rows.Count) rather than $($script:Phase12RequirementIds.Count)"
    $ids = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $rows) {
        Assert-Condition $ids.Add($row.requirement_id) "duplicate Phase 12 evidence requirement $($row.requirement_id)"
        Assert-Condition ($row.verified_commit -eq $Source) "Phase 12 evidence $($row.evidence_id) does not name the tested source"
        Assert-Condition ($row.result -eq 'passing' -and $row.configuration -match '^direct ') "Phase 12 evidence is not direct passing: $($row.evidence_id)"
        Assert-Condition ([string]::IsNullOrWhiteSpace($row.waiver)) "Phase 12 evidence has a waiver: $($row.evidence_id)"
    }
    foreach ($id in $script:Phase12RequirementIds) {
        Assert-Condition $ids.Contains($id) "Phase 12 direct evidence is absent for $id"
    }
}

function Assert-InheritedPhase12SourceEvidence {
    param([object[]]$Evidence, [string]$Source)
    $rows = @($Evidence | Where-Object { $_.evidence_id -like 'P12-*' })
    Assert-Condition ($rows.Count -eq $script:Phase12RequirementIds.Count) "correction source inherited $($rows.Count) Phase 12 rows rather than $($script:Phase12RequirementIds.Count)"
    $ids = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $rows) {
        Assert-Condition $ids.Add($row.requirement_id) "duplicate inherited Phase 12 evidence requirement $($row.requirement_id)"
        Assert-ExactCommit $row.verified_commit "inherited Phase 12 evidence"
        Assert-Condition ($row.verified_commit -eq $script:Phase12PreviousSource) "correction source inherited an unexpected Phase 12 source identity: $($row.evidence_id)"
        Assert-Condition ($row.verified_commit -ne $Source) "correction source already contains evidence claiming its own unvalidated identity: $($row.evidence_id)"
        Assert-Ancestor $row.verified_commit $Source "inherited Phase 12 evidence/source lineage"
        Assert-Condition ($row.result -eq 'passing' -and $row.configuration -match '^direct ') "inherited Phase 12 evidence is not direct passing: $($row.evidence_id)"
        Assert-Condition ([string]::IsNullOrWhiteSpace($row.waiver)) "inherited Phase 12 evidence has a waiver: $($row.evidence_id)"
    }
    foreach ($id in $script:Phase12RequirementIds) {
        Assert-Condition $ids.Contains($id) "correction source did not inherit Phase 12 evidence for $id"
    }
}

function Assert-InheritedPhase13SourceEvidence {
    param([object[]]$Evidence, [string]$Source)
    $rows = @($Evidence | Where-Object { $_.evidence_id -like 'P13-*' })
    Assert-Condition ($rows.Count -eq 46) "correction source inherited $($rows.Count) Phase 13 rows rather than 46"
    $ids = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $rows) {
        Assert-Condition $ids.Add($row.requirement_id) "duplicate inherited Phase 13 evidence requirement $($row.requirement_id)"
        Assert-ExactCommit $row.verified_commit "inherited Phase 13 evidence"
        Assert-Condition ($script:Phase13InheritedEvidenceSources -contains $row.verified_commit) "correction source inherited an unexpected Phase 13 source identity: $($row.evidence_id)"
        Assert-Condition ($row.verified_commit -ne $Source) "correction source already contains evidence claiming its own unvalidated identity: $($row.evidence_id)"
        Assert-Ancestor $row.verified_commit $Source "inherited Phase 13 evidence/source lineage"
        Assert-Condition ($row.result -eq 'passing' -and $row.configuration -match '^direct ') "inherited Phase 13 evidence is not direct passing: $($row.evidence_id)"
        Assert-Condition ([string]::IsNullOrWhiteSpace($row.waiver)) "inherited Phase 13 evidence has a waiver: $($row.evidence_id)"
    }
    Assert-Condition ($ids.Count -eq 46) "correction source did not inherit all Phase 13 evidence rows"
}

function Assert-Phase12ValidationRegistry {
    param([object[]]$Rows, [string]$Source)
    $phaseRows = @($Rows | Where-Object { $_.validation_id -like 'P12-VAL-*' })
    Assert-Condition ($phaseRows.Count -ge 16) "Phase 12 validation registry is incomplete"
    $commands = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $phaseRows) {
        Assert-Condition $commands.Add($row.exact_command) "duplicate Phase 12 validation command $($row.exact_command)"
        Assert-Condition ($row.working_directory -eq 'F:\projects\concord_api') "wrong Phase 12 validation working directory $($row.validation_id)"
        Assert-Condition ($row.source_commit -eq $Source) "Phase 12 validation used wrong source $($row.validation_id)"
        Assert-Condition ($row.exit_status -eq '0' -and $row.result -eq 'passing') "Phase 12 validation did not pass $($row.validation_id)"
        foreach ($field in @('tool_versions', 'feature_configuration', 'selected_tests', 'observed_at')) {
            Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.$field)) "Phase 12 validation omitted $field in $($row.validation_id)"
        }
    }
    foreach ($command in @(
        'just fmt-check', 'just check', 'just check-all', 'just check-no-default',
        'just clippy', 'just clippy-all', 'just test', 'just test-all',
        'just test-no-default', 'just doctest', 'just doctest-all', 'just doc',
        'just doc-all', 'just deny', 'just diff-check', 'just release'
    )) {
        Assert-Condition $commands.Contains($command) "required Phase 12 validation command was not recorded: $command"
    }
}

function Assert-Phase12EvidenceCommandsRecorded {
    param([object[]]$Evidence, [object[]]$Validation)
    $commands = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Validation | Where-Object { $_.validation_id -like 'P12-VAL-*' }) {
        [void]$commands.Add($row.exact_command)
    }
    foreach ($row in $Evidence | Where-Object { $_.evidence_id -like 'P12-*' }) {
        $proofs = @($row.exact_test -split '\s+;;\s+')
        Assert-Condition ($proofs.Count -gt 0) "Phase 12 evidence has no proof command: $($row.evidence_id)"
        foreach ($proof in $proofs) {
            Assert-Condition (-not [string]::IsNullOrWhiteSpace($proof)) "Phase 12 evidence has an empty proof command: $($row.evidence_id)"
            Assert-Condition $commands.Contains($proof) "Phase 12 evidence command lacks a passing validation record: $($row.evidence_id): $proof"
        }
    }
}

function Assert-Phase12HighRiskEvidenceAllocation {
    param([Parameter(Mandatory = $true)][object[]]$Rows)

    $required = @{
        "P12-SEC-RECORD-001" = @{
            Paths = @(
                "concord_core/src/records/error.rs",
                "concord_core/src/records/stream.rs",
                "concord_core/src/execute/tests.rs",
                "concord_core/tests/public_rustdoc.rs",
                "concord_core/tests/release/security_sentinels.csv",
                "concord_macros/tests/compile.rs"
            )
            Commands = @(
                "record_payloads_never_enter_hooks_debug_limiter_completion_or_sources",
                "record_surfaces_are_structural_private_and_value_free_in_rendered_rustdoc",
                "generated_records_rustdoc_uses_stable_paths_and_exposes_no_record_value_channel"
            )
        }
        "P12-SEC-RESP-003" = @{
            Paths = @(
                "concord_core/src/execute/tests.rs",
                "concord_core/src/response/error.rs",
                "concord_core/src/response/metadata.rs",
                "concord_core/src/response/stream.rs"
            )
            Commands = @(
                "response_payloads_never_enter_hooks_debug_limiter_completion_or_wrapper_debug",
                "formatting_and_sources_exclude_application_text",
                "debug_omits_application_value_url_and_every_header_value",
                "debug_is_non_consuming_and_drop_cancels_the_owned_body"
            )
        }
        "P12-EXEC-RECORD-001" = @{
            Paths = @(
                "concord_core/src/execute/tests.rs",
                "concord_core/src/records/stream.rs",
                "concord_core/src/response/handler.rs"
            )
            Commands = @(
                "records_response_validates_starts_unpolled_and_completes_stream_opened_once"
            )
        }
        "P12-EXEC-RECORD-005" = @{
            Paths = @(
                "concord_core/src/execute/tests.rs",
                "concord_core/src/records/stream.rs"
            )
            Commands = @(
                "record_stream_body_and_decoder_failures_are_items_without_second_completion",
                "dropping_record_stream_synchronously_drops_decoder_staging_and_byte_stream",
                "dropping_record_batches_synchronously_drops_record_stream_and_byte_stream"
            )
        }
        "P12-CONF-RECORD-002" = @{
            Paths = @(
                "concord_core/Cargo.toml",
                "concord_macros/Cargo.toml",
                "concord_core/tests/public_rustdoc.rs",
                "scripts/audit-record-feature-topology.ps1",
                "justfile"
            )
            Commands = @(
                "audit-record-feature-topology.ps1",
                "--features records",
                "--features csv",
                "--features ndjson",
                "rustdoc-records-only",
                "rustdoc-csv-only",
                "rustdoc-ndjson-only",
                "rustdoc-csv-ndjson",
                "cargo +1.85.0",
                "just deny"
            )
        }
        "P12-CONF-DOWNSTREAM-002" = @{
            Paths = @(
                "concord_macros/tests/fixtures/extension_certification/src/lib.rs",
                "concord_macros/tests/fixtures/extension_certification/tests/extension_workflows_runtime.rs",
                "concord_macros/tests/fixtures/extension_certification/tests/pagination_runtime.rs",
                "concord_macros/tests/compile.rs",
                "concord_core/tests/generic_spec_extensions.rs",
                "concord_core/tests/records_extensions.rs"
            )
            Commands = @(
                "downstream_credential_limiter_and_hooks_execute_through_public_runtime",
                "downstream_pagination_runtime_executes_generated_construction_and_binding",
                "downstream_request_and_all_sealed_response_modes_work_without_macro_changes",
                "downstream_decoder_streams_and_batches_through_generated_public_contracts"
            )
        }
    }

    foreach ($evidenceId in $required.Keys) {
        $matches = @($Rows | Where-Object { $_.evidence_id -eq $evidenceId })
        Assert-Condition ($matches.Count -eq 1) "high-risk Phase 12 evidence row is absent or duplicated: $evidenceId"
        $row = $matches[0]
        $paths = @($row.exact_path -split ";")
        foreach ($path in $required[$evidenceId].Paths) {
            Assert-Condition ($paths -contains $path) "$evidenceId omits principal path $path"
        }
        foreach ($commandFamily in $required[$evidenceId].Commands) {
            Assert-Condition ($row.exact_test.Contains($commandFamily)) "$evidenceId omits exact command family $commandFamily"
        }
    }
}

function Assert-Phase12StateDocumentation {
    param([string]$Commit, [string]$Source, [string]$EvidenceState, [string]$Anchor)
    $report = Get-GitBlobText $Commit $script:Phase12ReportPath
    $controller = Get-GitBlobText $Commit $script:ControllerPath
    foreach ($required in @(
        'PHASE_12_COMPLETE_WITH_LIMITATIONS', $Source, $EvidenceState, $Anchor,
        'generated contract `12 -> 13`', 'Phase 13', '`NOT_STARTED`',
        'Normative Implementation Candidate', 'Phase 14',
        '127 direct and 0 partial', '1,553 Phase 0-11 evidence rows'
    )) {
        Assert-Condition $report.Contains($required) "Phase 12 report is missing: $required"
    }
    foreach ($required in @(
        'latest_validated_phase = "12"', 'latest_verdict = "PHASE_12_COMPLETE_WITH_LIMITATIONS"',
        "source_commit = `"$Source`"", "evidence_commit = `"$EvidenceState`"",
        "anchor_commit = `"$Anchor`"", 'head_policy = "anchor-must-be-an-ancestor"',
        'normative_status = "Normative Implementation Candidate"',
        'id = "13"', 'title = "Lexically Scoped Rate Limits"',
        'status = "NOT_STARTED"'
    )) {
        Assert-Condition $controller.Contains($required) "Phase 12 controller state is missing: $required"
    }
}

function Resolve-EvidenceCommitValue {
    param([string]$Value, [string]$Evidence)
    if ($Value -eq $script:EvidenceToken) {
        return $Evidence
    }
    return $Value
}

function Get-OutsideNormativeStatements {
    param([hashtable]$Documents, [object[]]$Definitions)
    $statements = [System.Collections.Generic.List[object]]::new()
    foreach ($path in $script:NormativePaths) {
        $lines = @($Documents[$path] -split "`n", 0, "SimpleMatch")
        $covered = [System.Collections.Generic.HashSet[int]]::new()
        foreach ($definition in $Definitions | Where-Object normative_document -eq $path) {
            for ($line = $definition.start_line; $line -le $definition.end_line; $line++) {
                [void]$covered.Add($line)
            }
        }
        $buffer = [System.Collections.Generic.List[string]]::new()
        $flush = {
            if ($buffer.Count -eq 0) { return }
            $text = Normalize-Prose ($buffer -join " ")
            $buffer.Clear()
            $masked = [regex]::Replace($text, '`[^`]*`', '')
            if ([regex]::IsMatch($masked, '\b(?:MUST NOT|MUST|SHOULD NOT|SHOULD|MAY)\b')) {
                $statements.Add([pscustomobject]@{
                    normative_document = $path
                    normalized_statement = $text
                    normalized_statement_digest = Get-TextSha256 $text
                })
            }
        }
        $inFence = $false
        for ($index = 0; $index -lt $lines.Count; $index++) {
            $lineNumber = $index + 1
            $line = $lines[$index]
            if ($covered.Contains($lineNumber)) {
                & $flush
                continue
            }
            if ($line -match '^```') {
                & $flush
                $inFence = -not $inFence
                continue
            }
            if ($inFence -or $line -match '^\s*(?:#|>|\|)') {
                & $flush
                continue
            }
            if ([string]::IsNullOrWhiteSpace($line)) {
                & $flush
                continue
            }
            if ($line -match '^\s*(?:-|[0-9]+\.)\s+') {
                & $flush
                $buffer.Add($line.Trim())
                & $flush
                continue
            }
            $buffer.Add($line.Trim())
        }
        & $flush
    }
    return @($statements)
}

function Assert-NormativeLanguage {
    param([object[]]$Statements, [object[]]$Allowlist)
    $allowed = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Allowlist) {
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.normative_document)) "language allowlist has no document"
        Assert-Condition ($row.classification -eq "normative-language-definition") "invalid language allowlist classification"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.reason)) "language allowlist entry has no reason"
        $key = "$($row.normative_document)|$($row.normalized_statement_digest)"
        Assert-Condition $allowed.Add($key) "duplicate language allowlist entry $key"
    }
    $observed = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($statement in $Statements) {
        $key = "$($statement.normative_document)|$($statement.normalized_statement_digest)"
        Assert-Condition $allowed.Contains($key) "untracked normative prose in $($statement.normative_document): $($statement.normalized_statement)"
        [void]$observed.Add($key)
    }
    foreach ($key in $allowed) {
        Assert-Condition $observed.Contains($key) "stale normative-language allowlist entry $key"
    }
}

function Assert-AcyclicDependencies {
    param([object[]]$Rows)
    $nodes = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $edges = @{}
    $indegree = @{}
    foreach ($row in $Rows) {
        [void]$nodes.Add($row.dependent_document)
        [void]$nodes.Add($row.authority_document)
    }
    foreach ($node in $nodes) {
        $edges[$node] = [System.Collections.Generic.List[string]]::new()
        $indegree[$node] = 0
    }
    foreach ($row in $Rows) {
        Assert-Condition ($row.dependent_document -ne $row.authority_document) "self dependency $($row.dependent_document)"
        $edges[$row.dependent_document].Add($row.authority_document)
        $indegree[$row.authority_document]++
    }
    $queue = [System.Collections.Generic.Queue[string]]::new()
    foreach ($node in $nodes) {
        if ($indegree[$node] -eq 0) { $queue.Enqueue($node) }
    }
    $visited = 0
    while ($queue.Count -gt 0) {
        $node = $queue.Dequeue()
        $visited++
        foreach ($next in $edges[$node]) {
            $indegree[$next]--
            if ($indegree[$next] -eq 0) { $queue.Enqueue($next) }
        }
    }
    Assert-Condition ($visited -eq $nodes.Count) "normative dependency cycle detected"
}

function Assert-DependencyRegistry {
    param([object[]]$Rows, [object[]]$Definitions)
    $expected = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $names = @($script:NormativePaths | ForEach-Object { Split-Path $_ -Leaf })
    $pairs = @(
        @(1, 0), @(2, 0), @(2, 1), @(3, 0), @(3, 1),
        @(4, 0), @(4, 1), @(4, 2), @(4, 3),
        @(5, 0), @(5, 1), @(5, 3),
        @(6, 0), @(6, 1), @(6, 3), @(6, 5),
        @(7, 0), @(7, 1), @(7, 3), @(7, 5), @(7, 6),
        @(8, 0), @(8, 1), @(8, 2), @(8, 3), @(8, 4), @(8, 5), @(8, 6), @(8, 7)
    )
    foreach ($pair in $pairs) { [void]$expected.Add("$($names[$pair[0]])|$($names[$pair[1]])") }
    $actual = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $definitionIds = [System.Collections.Generic.HashSet[string]]::new([string[]]$Definitions.requirement_id, [System.StringComparer]::Ordinal)
    foreach ($row in $Rows) {
        $key = "$($row.dependent_document)|$($row.authority_document)"
        Assert-Condition $actual.Add($key) "duplicate normative dependency $key"
        Assert-Condition $expected.Contains($key) "unapproved normative dependency $key"
        Assert-Condition ($row.owner_reference -eq "DOC-DEP-001" -and $definitionIds.Contains($row.owner_reference)) "invalid dependency owner reference in $key"
        $dependentNumber = [int]$row.dependent_document.Substring(0, 2)
        $authorityNumber = [int]$row.authority_document.Substring(0, 2)
        Assert-Condition ($dependentNumber -gt $authorityNumber) "dependency points against authority order: $key"
        Assert-Condition ($row.result -eq "passing") "dependency review did not pass: $key"
    }
    Assert-Condition ($actual.Count -eq $expected.Count) "normative dependency registry is incomplete"
    Assert-AcyclicDependencies $Rows
}

function Assert-OwnershipRegistry {
    param([object[]]$Rows, [object[]]$Definitions)
    $expected = @{
        public_signatures = @("03_CONCORD_PUBLIC_API_SPEC_0.1.md", "API-")
        dsl_productions = @("02_CONCORD_DSL_SPEC_0.1.md", "DSL-")
        generated_naming = @("04_CONCORD_GENERATED_API_SPEC_0.1.md", "GEN-")
        finite_limits = @("05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md", "SEC-;BOUND-")
        lifecycle_ordering = @("06_CONCORD_EXECUTION_SPEC_0.1.md", "EXEC-")
        feature_and_compatibility = @("07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md", "FEAT-;COMPAT-")
    }
    $subjects = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Rows) {
        Assert-Condition $subjects.Add($row.subject) "duplicate ownership subject $($row.subject)"
        Assert-Condition $expected.ContainsKey($row.subject) "unknown ownership subject $($row.subject)"
        Assert-Condition ($row.authoritative_document -eq $expected[$row.subject][0]) "wrong owner for $($row.subject)"
        Assert-Condition ($row.requirement_prefixes -eq $expected[$row.subject][1]) "wrong prefixes for $($row.subject)"
        Assert-Condition ($row.audit_rule -eq "unique-prefix-owner-and-language-audit") "wrong ownership audit rule for $($row.subject)"
        Assert-Condition ($row.result -eq "passing") "ownership review did not pass for $($row.subject)"
        foreach ($prefix in $row.requirement_prefixes -split ';') {
            foreach ($definition in $Definitions | Where-Object requirement_id -like "$prefix*") {
                Assert-Condition ((Split-Path $definition.normative_document -Leaf) -eq $row.authoritative_document) "duplicate or wrong normative owner for $($definition.requirement_id)"
            }
        }
    }
    Assert-Condition ($subjects.Count -eq $expected.Count) "ownership registry is incomplete"
}

function Assert-RequirementAndClaimRegistries {
    param(
        [object[]]$Definitions,
        [object[]]$Evidence,
        [object[]]$RequirementRows,
        [object[]]$ClaimRows,
        [string]$Source,
        [string]$EvidenceState,
        [string]$Historical
    )
    Assert-Condition ($RequirementRows.Count -eq 504) "release requirement registry contains $($RequirementRows.Count) rows rather than 504"
    $definitionsById = @{}
    foreach ($definition in $Definitions) { $definitionsById[$definition.requirement_id] = $definition }
    $evidenceById = @{}
    foreach ($row in $Evidence) { $evidenceById[$row.evidence_id] = $row }
    $requirementsById = @{}
    $isAncestor = { param($left, $right) & git merge-base --is-ancestor $left $right 2>$null; return $LASTEXITCODE -eq 0 }
    foreach ($row in $RequirementRows) {
        Assert-Condition (-not $requirementsById.ContainsKey($row.requirement_id)) "duplicate release requirement $($row.requirement_id)"
        Assert-Condition $definitionsById.ContainsKey($row.requirement_id) "unknown release requirement $($row.requirement_id)"
        $definition = $definitionsById[$row.requirement_id]
        Assert-RequirementMetadata $row $definition
        Assert-Condition $evidenceById.ContainsKey($row.evidence_id) "unknown evidence owner $($row.evidence_id)"
        $owner = $evidenceById[$row.evidence_id]
        Assert-Condition ($owner.requirement_id -eq $row.requirement_id) "evidence owner mismatch for $($row.requirement_id)"
        Assert-Condition ($owner.result -eq "passing" -and $owner.configuration -match '^direct ') "release owner is not direct passing: $($row.evidence_id)"
        Assert-Condition ($row.result -eq "passing") "release requirement did not pass: $($row.requirement_id)"
        Assert-NoWaiver $row.result $row.waiver_id $row.requirement_id
        $verified = Resolve-EvidenceCommitValue $owner.verified_commit $EvidenceState
        Assert-CommitClass $verified $row.evidence_commit_class $Source $EvidenceState $Historical $isAncestor $row.evidence_id
        $expectedOwner = switch ($row.evidence_commit_class) {
            "immutable_source" { "Phase 10 corrected immutable source" }
            "evidence_state" { "Phase 10 corrected evidence state" }
            "historical" { "Historical validated evidence" }
        }
        Assert-Condition ($row.current_direct_passing_evidence_owner -eq $expectedOwner) "wrong evidence-owner label for $($row.requirement_id)"
        $requirementsById[$row.requirement_id] = $row
    }

    $expectedClaims = @(Get-ExpectedClaims $Definitions)
    $expectedById = @{}
    foreach ($claim in $expectedClaims) { $expectedById[$claim.claim_id] = $claim }
    Assert-Condition ($ClaimRows.Count -eq $expectedClaims.Count) "claim registry contains $($ClaimRows.Count) rows rather than recomputed $($expectedClaims.Count)"
    $claimIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $requirementsWithClaims = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($claim in $ClaimRows) {
        Assert-Condition $claimIds.Add($claim.claim_id) "duplicate claim ID $($claim.claim_id)"
        Assert-Condition $expectedById.ContainsKey($claim.claim_id) "unknown claim ID $($claim.claim_id)"
        $expectedClaim = $expectedById[$claim.claim_id]
        Assert-Condition ($claim.requirement_id -eq $expectedClaim.requirement_id) "claim requirement mismatch $($claim.claim_id)"
        Assert-Condition ($claim.claim_atom -eq $expectedClaim.claim_atom) "claim atom mismatch $($claim.claim_id)"
        Assert-Condition $requirementsById.ContainsKey($claim.requirement_id) "claim has no release requirement $($claim.claim_id)"
        $requirement = $requirementsById[$claim.requirement_id]
        $definition = $definitionsById[$claim.requirement_id]
        Assert-Condition ($claim.normative_document -eq $definition.normative_document) "claim document mismatch $($claim.claim_id)"
        Assert-Condition ($claim.normative_title -eq $definition.normative_title) "claim title mismatch $($claim.claim_id)"
        Assert-Condition ($claim.normative_paragraph_digest -eq $definition.normative_paragraph_digest) "claim digest mismatch $($claim.claim_id)"
        Assert-Condition ($claim.evidence_id -eq $requirement.evidence_id) "claim evidence mismatch $($claim.claim_id)"
        $owner = $evidenceById[$claim.evidence_id]
        Assert-Condition ($claim.classification -eq "direct") "claim is not direct $($claim.claim_id)"
        Assert-Condition ($claim.proof_path -eq $owner.exact_path) "claim proof path mismatch $($claim.claim_id)"
        Assert-Condition ($claim.proof_command -eq $owner.exact_test) "claim proof command mismatch $($claim.claim_id)"
        Assert-Condition ((Resolve-EvidenceCommitValue $claim.proof_commit $EvidenceState) -eq (Resolve-EvidenceCommitValue $owner.verified_commit $EvidenceState)) "claim proof commit mismatch $($claim.claim_id)"
        Assert-CompleteClaim $claim
        [void]$requirementsWithClaims.Add($claim.requirement_id)
    }
    Assert-Condition ($requirementsWithClaims.Count -eq $Definitions.Count) "one or more release requirements have no claim atom"
    return $expectedClaims.Count
}

function Assert-ChangeMap {
    param([object[]]$Rows, [object[]]$Definitions, [string]$Source, [string]$EvidenceState)
    $knownIds = [System.Collections.Generic.HashSet[string]]::new([string[]]$Definitions.requirement_id, [System.StringComparer]::Ordinal)
    $expectedPaths = @(& git diff --name-only "$HistoricalBaseline..$EvidenceState")
    Assert-Condition ($LASTEXITCODE -eq 0) "unable to derive release change set"
    $sourcePaths = [System.Collections.Generic.HashSet[string]]::new(
        [string[]]@(& git diff --name-only "$HistoricalBaseline..$Source"),
        [System.StringComparer]::Ordinal
    )
    Assert-Condition ($LASTEXITCODE -eq 0) "unable to derive source change set"
    $evidenceStatePaths = [System.Collections.Generic.HashSet[string]]::new(
        [string[]]@(& git diff --name-only "$Source..$EvidenceState"),
        [System.StringComparer]::Ordinal
    )
    Assert-Condition ($LASTEXITCODE -eq 0) "unable to derive evidence-state change set"
    $actualPaths = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $changeIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Rows) {
        Assert-Condition $changeIds.Add($row.change_id) "duplicate release change ID $($row.change_id)"
        Assert-Condition $actualPaths.Add($row.exact_path) "duplicate release change path $($row.exact_path)"
        Assert-DurablePath $row.exact_path $row.change_id
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.observable_change)) "release change has no description $($row.change_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.proof_command)) "release change has no proof command $($row.change_id)"
        Assert-Condition ($row.result -eq "passing") "release change did not pass $($row.change_id)"
        $expectedClass = if ($evidenceStatePaths.Contains($row.exact_path)) {
            "evidence_state"
        } else {
            Assert-Condition $sourcePaths.Contains($row.exact_path) "change path has no source or evidence-state owner: $($row.exact_path)"
            "immutable_source"
        }
        Assert-Condition ($row.proof_commit_class -eq $expectedClass) "wrong change commit class for $($row.exact_path)"
        $ids = @($row.requirement_ids -split ';')
        Assert-Condition ($ids.Count -gt 0 -and $ids -contains "DOC-CHG-001") "change lacks DOC-CHG-001 mapping: $($row.exact_path)"
        foreach ($id in $ids) { Assert-Condition $knownIds.Contains($id) "change maps to unknown requirement $id" }
    }
    Assert-Condition ($actualPaths.Count -eq $expectedPaths.Count) "release change map count mismatch: $($actualPaths.Count) vs $($expectedPaths.Count)"
    foreach ($path in $expectedPaths) { Assert-Condition $actualPaths.Contains($path) "unmapped release change $path" }
}

function Assert-ValidationRegistry {
    param([object[]]$Rows, [string]$Source)
    $commands = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Rows) {
        $commandIdentity = "$($row.exact_command)|$($row.source_commit)"
        Assert-Condition $commands.Add($commandIdentity) "duplicate validation command/source $($row.exact_command) / $($row.source_commit)"
        Assert-Condition ($row.working_directory -eq "F:\projects\concord_api") "wrong validation working directory $($row.validation_id)"
        Assert-Condition ($row.source_commit -eq $Source) "validation used wrong source $($row.validation_id)"
        Assert-Condition ($row.exit_status -eq "0" -and $row.result -eq "passing") "validation did not pass $($row.validation_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.tool_versions)) "validation omitted tool versions $($row.validation_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.feature_configuration)) "validation omitted feature configuration $($row.validation_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.selected_tests)) "validation omitted selected tests $($row.validation_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.observed_at)) "validation omitted timestamp $($row.validation_id)"
    }
    $required = @(
        "cargo nextest run --profile ci -p concord_core --all-features --test development -E 'test(=benchmark_operations::benchmark_seams_execute_head_preparation_and_ready_credential_finalization)'",
        "cargo bench -p concord_perf --bench concord_overhead --no-run",
        "cargo bench -p concord_perf --bench concord_overhead -- --smoke",
        "just _test-dangerous-features", "just _feature-matrix", "just _examples", "just _benchmarks", "just _msrv",
        "just _grammar-and-diagnostics", "just _reqwest-assumptions", "just _security-and-properties",
        "just _rustdoc-audits", "just _release-audits",
        "just fmt-check", "just check", "just check-all", "just check-no-default", "just clippy", "just clippy-all",
        "just test", "just test-all", "just test-no-default", "just doctest", "just doctest-all", "just doc",
        "just doc-all", "just deny", "just diff-check", "just release"
    )
    foreach ($command in $required) { Assert-Condition $commands.Contains($command) "required validation command was not recorded: $command" }
}

function Assert-ReleaseDocumentation {
    param([string]$StateCommit)
    $readme = Get-GitBlobText $StateCommit "README.md"
    $docsReadme = Get-GitBlobText $StateCommit "docs/README.md"
    $report = Get-GitBlobText $StateCommit "docs/conformance/PHASE_10_REPORT.md"
    $combined = "$readme`n$docsReadme`n$report"
    Assert-Condition ($combined -notmatch 'Phase 10 is not started|PHASE_10_NOT_STARTED|positive raw (?:response|success).*absent') "transitional Phase 10 documentation remains"
    Assert-Condition ($readme -match 'binary-wide|binary wide' -and $readme -match 'feature unification') "README omits binary-wide Cargo feature unification"
    Assert-Condition ($readme -match 'pre-1\.0' -and $readme -match 'patch') "README omits the pre-1.0 patch compatibility policy"
    Assert-Condition ($combined -match 'locally validated release candidate' -and $combined -match 'not (?:a )?published release') "release-candidate terminology is incomplete"
    Assert-Condition ($combined -notmatch '(?i)published to crates\.io|deployed release|tagged release') "documentation claims an external release operation"
}

function Assert-SubstitutionCommit {
    param([string]$Before, [string]$After, [string]$Token, [string]$Replacement, [string[]]$AllowedPaths)
    $changed = @(& git diff --name-only "$Before..$After")
    Assert-Condition ($LASTEXITCODE -eq 0 -and $changed.Count -gt 0) "substitution commit has no changes"
    foreach ($path in $changed) {
        Assert-Condition ($AllowedPaths -contains $path) "unapproved substitution path $path"
        $beforeText = Get-GitBlobText $Before $path
        $afterText = Get-GitBlobText $After $path
        Assert-Condition $beforeText.Contains($Token) "substitution source lacks $Token in $path"
        Assert-Condition (-not $afterText.Contains($Token)) "substitution token remains in $path"
        Assert-Condition ($afterText.Replace($Replacement, $Token) -ceq $beforeText) "substitution commit changed more than identity text in $path"
    }
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$Name)
    $threw = $false
    try { & $Action } catch { $threw = $true }
    Assert-Condition $threw "self-test expected rejection: $Name"
}

function Invoke-SelfTests {
    $head = (& git rev-parse HEAD).Trim()
    Assert-ExactCommit $head "self-test HEAD"
    $documents = Get-NormativeDocuments $head
    $parsedDefinitions = @(Get-NormativeDefinitions $documents)
    $parsedClaims = @(Get-ExpectedClaims $parsedDefinitions)
    Assert-Condition ($parsedDefinitions.Count -eq $script:ActiveRequirementCount) "self-test normative parser did not find $($script:ActiveRequirementCount) active requirements"
    Assert-Condition ($parsedClaims.Count -gt $script:ActiveRequirementCount) "self-test claim parser did not decompose compound requirements"
    $fileType = (& git cat-file -t "$head`:Cargo.toml").Trim()
    $directoryType = (& git cat-file -t "$head`:concord_core/src").Trim()
    Assert-Condition ($fileType -eq "blob") "self-test did not accept a valid file"
    Assert-Condition ($directoryType -eq "tree") "self-test did not accept a valid directory"
    Assert-Throws { $type = @(& git cat-file -t "$head`:missing-release-audit-path" 2>$null); Assert-Condition ($LASTEXITCODE -eq 0) "missing" } "missing path"
    Assert-Throws { Assert-DurablePath "git_diff/ignored.md" "SELFTEST" } "ignored git_diff path"

    $prospectiveSource = "1111111111111111111111111111111111111111"
    $alwaysAncestor = { param($left, $right) return $true }
    $neverAncestor = { param($left, $right) return $false }
    Assert-Phase13CorrectionBaselineInput $script:Phase13ProspectiveCorrectionBaseline $prospectiveSource $alwaysAncestor
    Assert-Throws {
        Assert-Phase13CorrectionBaselineInput "" $prospectiveSource $alwaysAncestor
    } "missing Phase 13 correction baseline"
    Assert-Throws {
        Assert-Phase13CorrectionBaselineInput $script:Phase13ProspectiveCorrectionBaseline $prospectiveSource $neverAncestor
    } "non-ancestral Phase 13 correction baseline"
    Assert-Phase13SourceRangePaths @("scripts/audit-release-registry.ps1")
    foreach ($forbiddenSourcePath in @(
        $script:EvidencePath,
        $script:Phase13ReportPath,
        $script:ControllerPath,
        "README.md"
    )) {
        Assert-Throws {
            Assert-Phase13SourceRangePaths @("scripts/audit-release-registry.ps1", $forbiddenSourcePath)
        } "prospective source range containing $forbiddenSourcePath"
    }
    Assert-Phase13PostSourcePaths @(
        $script:EvidencePath,
        $script:ValidationRegistryPath,
        $script:Phase13ReportPath,
        $script:ControllerPath
    )
    Assert-Throws {
        Assert-Phase13PostSourcePaths @($script:Phase13ReportPath, "scripts/audit-release-registry.ps1")
    } "post-source executable change"
    $inheritedPhase13Evidence = @(Import-GitCsv $head $script:EvidencePath)
    Assert-InheritedPhase13SourceEvidence $inheritedPhase13Evidence $head

    Assert-CommitClass "a" "immutable_source" "a" "b" "h" $alwaysAncestor "SOURCE"
    Assert-CommitClass "b" "evidence_state" "a" "b" "h" $alwaysAncestor "EVIDENCE"
    Assert-Throws { Assert-CommitClass "b" "immutable_source" "a" "b" "h" $alwaysAncestor "WRONG-SOURCE" } "source identity class"
    Assert-Throws { Assert-CommitClass "a" "evidence_state" "a" "b" "h" $alwaysAncestor "WRONG-EVIDENCE" } "evidence identity class"
    $sourceChangePaths = [System.Collections.Generic.HashSet[string]]::new([string[]]@("source-only", "changed-again"), [System.StringComparer]::Ordinal)
    $evidenceChangePaths = [System.Collections.Generic.HashSet[string]]::new([string[]]@("evidence-only", "changed-again"), [System.StringComparer]::Ordinal)
    $sourceOnlyClass = if ($evidenceChangePaths.Contains("source-only")) { "evidence_state" } elseif ($sourceChangePaths.Contains("source-only")) { "immutable_source" }
    $changedAgainClass = if ($evidenceChangePaths.Contains("changed-again")) { "evidence_state" } elseif ($sourceChangePaths.Contains("changed-again")) { "immutable_source" }
    Assert-Condition ($sourceOnlyClass -eq "immutable_source") "source-only change classification failed"
    Assert-Condition ($changedAgainClass -eq "evidence_state") "post-source change did not take evidence-state precedence"
    Assert-Throws { Assert-CommitClass "x" "historical" "a" "b" "h" $neverAncestor "NON-ANCESTOR" } "non-ancestral commit"

    $definition = [pscustomobject]@{ requirement_id = "TEST-REQ-001"; normative_document = "doc.md"; normative_title = "Expected"; normative_paragraph_digest = "digest" }
    $evidence = [pscustomobject]@{ requirement_id = "TEST-REQ-001"; evidence_id = "TEST-EVIDENCE"; result = "passing"; configuration = "direct self-test"; verified_commit = "a"; exact_path = "Cargo.toml"; exact_test = "test" }
    $requirement = [pscustomobject]@{ requirement_id = "TEST-REQ-001"; normative_document = "doc.md"; normative_title = "Wrong"; normative_paragraph_digest = "digest"; evidence_id = "TEST-EVIDENCE"; evidence_commit_class = "immutable_source"; current_direct_passing_evidence_owner = "Phase 10 corrected immutable source"; result = "passing"; waiver_id = "" }
    Assert-Throws { Assert-RequirementMetadata $requirement $definition } "normative title mismatch"
    $requirement.normative_title = "Expected"
    $requirement.normative_paragraph_digest = "wrong"
    Assert-Throws { Assert-RequirementMetadata $requirement $definition } "paragraph digest mismatch"

    $known = [System.Collections.Generic.HashSet[string]]::new([string[]]@("TEST-REQ-001"), [System.StringComparer]::Ordinal)
    Assert-Throws { Assert-KnownRequirement "UNKNOWN-001" $known "self-test" } "unknown requirement"
    Assert-Throws { Assert-NoWaiver "waived" "unapproved" "self-test" } "unapproved waiver"

    $untracked = [pscustomobject]@{ normative_document = "doc.md"; normalized_statement = "Behavior MUST drift."; normalized_statement_digest = "x" }
    Assert-Throws { Assert-NormativeLanguage @($untracked) @() } "untracked normative prose"
    $cycle = @(
        [pscustomobject]@{ dependent_document = "A"; authority_document = "B" },
        [pscustomobject]@{ dependent_document = "B"; authority_document = "A" }
    )
    Assert-Throws { Assert-AcyclicDependencies $cycle } "normative dependency cycle"
    $duplicates = @(
        [pscustomobject]@{ subject = "public_signatures"; authoritative_document = "03_CONCORD_PUBLIC_API_SPEC_0.1.md"; requirement_prefixes = "API-"; audit_rule = "unique-prefix-owner-and-language-audit"; result = "passing" },
        [pscustomobject]@{ subject = "public_signatures"; authoritative_document = "02_CONCORD_DSL_SPEC_0.1.md"; requirement_prefixes = "DSL-"; audit_rule = "unique-prefix-owner-and-language-audit"; result = "passing" }
    )
    Assert-Throws { Assert-OwnershipRegistry $duplicates @() } "duplicate ownership"
    $incompleteClaim = [pscustomobject]@{ claim_id = "X"; requirement_id = "TEST-REQ-001"; claim_atom = ""; result = "passing"; later_owner = "later" }
    Assert-Throws { Assert-CompleteClaim $incompleteClaim } "incomplete claim atom"
    $commandEvidence = [pscustomobject]@{ evidence_id = "P12-TEST"; exact_test = "focused command ;; missing command" }
    $commandValidation = [pscustomobject]@{ validation_id = "P12-VAL-TEST"; exact_command = "focused command" }
    Assert-Throws {
        Assert-Phase12EvidenceCommandsRecorded @($commandEvidence) @($commandValidation)
    } "unrecorded Phase 12 evidence command"
    $completeCommandValidation = @(
        $commandValidation,
        [pscustomobject]@{ validation_id = "P12-VAL-TEST-2"; exact_command = "missing command" }
    )
    Assert-Phase12EvidenceCommandsRecorded @($commandEvidence) $completeCommandValidation
    $expectedRequirements = [System.Collections.Generic.HashSet[string]]::new([string[]]@("A", "B"), [System.StringComparer]::Ordinal)
    $resolvedRequirements = [System.Collections.Generic.HashSet[string]]::new([string[]]@("A"), [System.StringComparer]::Ordinal)
    Assert-Throws {
        foreach ($id in $expectedRequirements) { Assert-Condition $resolvedRequirements.Contains($id) "unresolved release requirement $id" }
    } "unresolved release requirement"
    Write-Output "release registry audit self-tests passed: 26 rejection/acceptance classes; parsed-claims=$($parsedClaims.Count)"
}

function Test-Phase14SourcePath {
    param([string]$Path)
    return (
        $Path -eq "justfile" -or
        $Path -eq ".config/nextest.toml" -or
        $Path -eq "Cargo.toml" -or
        $Path -eq "Cargo.lock" -or
        $Path -eq "rust-toolchain.toml" -or
        $Path -eq "deny.toml" -or
        $Path -match '^scripts/[^/]+\.ps1$' -or
        $Path -match '^(?:concord_core|concord_macros|concord_examples|concord_test_support|perf)/(?:Cargo\.toml|src/|tests/|examples/|benches/)' -or
        $Path -match '^concord_(?:core|macros)/tests/fixtures/.+/Cargo\.lock$'
    )
}

function Assert-Phase14SourcePaths {
    param([string[]]$Paths)
    $actual = @($Paths | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    Assert-Condition ($actual.Count -gt 0) "Phase 14 source range is empty"
    foreach ($path in $actual) {
        Assert-Condition (Test-Phase14SourcePath $path) "Phase 14 source range contains non-executable or unapproved state: $path"
    }
}

function Assert-Phase14BaselineInput {
    param([string]$Baseline, [string]$Source, [scriptblock]$AncestorCheck)
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($Baseline)) "Phase 14 requires an explicit PhaseBaseline"
    Assert-Condition ($Baseline -eq $script:Phase14Baseline) "unexpected Phase 14 baseline: $Baseline"
    Assert-Condition ($Baseline -ne $Source) "Phase 14 baseline must precede SourceCommit"
    Assert-Condition (& $AncestorCheck $Baseline $Source) "Phase 14 baseline is not ancestral to SourceCommit: $Baseline -> $Source"
}

function Test-Phase14PostSourcePath {
    param([string]$Path)
    return (
        $Path -eq "README.md" -or
        $Path -eq "docs/README.md" -or
        $Path -eq "docs/REVIEW_RESOLUTION_0.1.md" -or
        $Path -eq "docs/decisions/CONCORD_DECISIONS.md" -or
        $Path -eq "docs/planning/CONCORD_IMPLEMENTATION_PROGRAM_0.2.md" -or
        $Path -eq $script:ControllerPath -or
        $Path -eq $script:Phase14ReportPath -or
        $Path -in @(
            $script:EvidencePath,
            $script:CoveragePath,
            $script:RequirementRegistryPath,
            $script:ClaimRegistryPath,
            $script:ChangeMapPath,
            $script:LanguageAllowlistPath,
            $script:DependencyRegistryPath,
            $script:OwnershipRegistryPath,
            $script:ValidationRegistryPath
        ) -or
        $Path -in $script:IndexedNormativePaths -or
        $Path -in $script:ArchitecturePaths
    )
}

function Assert-Phase14PostSourcePaths {
    param([string[]]$Paths)
    foreach ($path in @($Paths | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })) {
        Assert-Condition (Test-Phase14PostSourcePath $path) "post-source executable or unapproved release-state change: $path"
    }
}

function Assert-Phase14Boundary {
    param([string]$Baseline, [string]$Source)
    $gitAncestor = {
        param($left, $right)
        & git merge-base --is-ancestor $left $right 2>$null
        return $LASTEXITCODE -eq 0
    }
    Assert-Phase14BaselineInput $Baseline $Source $gitAncestor
    Assert-ExactCommit $Baseline "PhaseBaseline"
    $paths = @(& git diff --name-only "$Baseline..$Source")
    Assert-Condition ($LASTEXITCODE -eq 0) "Phase 14 source-range inspection failed"
    Assert-Phase14SourcePaths $paths
}

function Assert-BytePrefix {
    param(
        [byte[]]$Prefix,
        [byte[]]$Current,
        [string]$Description
    )
    Assert-Condition ($Current.Length -ge $Prefix.Length) "$Description is shorter than its historical prefix"
    for ($index = 0; $index -lt $Prefix.Length; $index++) {
        if ($Prefix[$index] -ne $Current[$index]) {
            throw "$Description differs from its historical prefix at byte $index"
        }
    }
}

function Assert-Phase14HistoricalPrefixes {
    param([string]$StateCommit)
    $historicalEvidence = [byte[]](Get-GitBlobBytes $script:Phase14Baseline $script:EvidencePath)
    Assert-Condition ($historicalEvidence.Length -eq $script:Phase14HistoricalEvidenceBytes) "Phase 0-13 evidence byte count changed at the Phase 14 baseline"
    Assert-Condition ((Get-Sha256Hex $historicalEvidence) -eq $script:Phase14HistoricalEvidenceHash) "Phase 0-13 evidence hash changed at the Phase 14 baseline"
    $historicalEvidenceRows = @(Get-GitBlobText $script:Phase14Baseline $script:EvidencePath | ConvertFrom-Csv)
    Assert-Condition ($historicalEvidenceRows.Count -eq $script:Phase14HistoricalEvidenceRows) "Phase 0-13 evidence row count changed at the Phase 14 baseline"
    Assert-BytePrefix $historicalEvidence ([byte[]](Get-GitBlobBytes $StateCommit $script:EvidencePath)) "Phase 0-13 evidence catalog"

    $historicalValidation = [byte[]](Get-GitBlobBytes $script:Phase14Baseline $script:ValidationRegistryPath)
    Assert-Condition ((Get-Sha256Hex $historicalValidation) -eq $script:Phase14HistoricalValidationHash) "historical validation registry hash changed at the Phase 14 baseline"
    $historicalValidationRows = @(Get-GitBlobText $script:Phase14Baseline $script:ValidationRegistryPath | ConvertFrom-Csv)
    Assert-Condition ($historicalValidationRows.Count -eq $script:Phase14HistoricalValidationRows) "historical validation row count changed at the Phase 14 baseline"
    Assert-BytePrefix $historicalValidation ([byte[]](Get-GitBlobBytes $StateCommit $script:ValidationRegistryPath)) "Phase 12-13 validation registry"
}

function Assert-Phase14ReportsPreserved {
    param([string]$StateCommit)
    for ($phase = 0; $phase -le 13; $phase++) {
        $path = "docs/conformance/PHASE_$($phase)_REPORT.md"
        & git diff --quiet $script:Phase14Baseline $StateCommit -- $path
        Assert-Condition ($LASTEXITCODE -eq 0) "historical report changed during Phase 14: $path"
    }
}

function Assert-Phase14NormativeParagraphsPreserved {
    param([object[]]$Definitions)
    $baselineDocuments = Get-NormativeDocuments $script:Phase14Baseline
    $baselineDefinitions = @(Get-NormativeDefinitions $baselineDocuments)
    Assert-Condition ($baselineDefinitions.Count -eq $Definitions.Count) "normative definition count changed during Phase 14"
    $currentById = @{}
    foreach ($definition in $Definitions) { $currentById[$definition.requirement_id] = $definition }
    foreach ($baseline in $baselineDefinitions) {
        Assert-Condition $currentById.ContainsKey($baseline.requirement_id) "normative requirement disappeared during Phase 14: $($baseline.requirement_id)"
        $current = $currentById[$baseline.requirement_id]
        Assert-Condition ($current.normative_document -eq $baseline.normative_document) "normative owner changed during Phase 14: $($baseline.requirement_id)"
        Assert-Condition ($current.normative_title -eq $baseline.normative_title) "normative title changed during Phase 14: $($baseline.requirement_id)"
        Assert-Condition ($current.normative_paragraph_digest -eq $baseline.normative_paragraph_digest) "normative requirement prose changed during Phase 14: $($baseline.requirement_id)"
    }
}

function Assert-Phase14CandidateState {
    param([string]$Commit)
    foreach ($path in $script:IndexedNormativePaths) {
        $text = Get-GitBlobText $Commit $path
        Assert-Condition $text.Contains('**Status:** Normative Implementation Candidate') "source mode inherited a non-candidate normative status: $path"
    }
    foreach ($path in $script:ArchitecturePaths) {
        $text = Get-GitBlobText $Commit $path
        Assert-Condition $text.Contains('**Status:** Internal Implementation Candidate') "source mode inherited a promoted architecture status: $path"
    }
    $controller = Get-GitBlobText $Commit $script:ControllerPath
    foreach ($required in @(
        'latest_validated_phase = "13"',
        'latest_verdict = "PHASE_13_COMPLETE_WITH_LIMITATIONS"',
        'normative_status = "Normative Implementation Candidate"',
        'id = "14"',
        'status = "NOT_STARTED"'
    )) {
        Assert-Condition $controller.Contains($required) "source mode controller state mismatch: $required"
    }
}

function Assert-Phase14ReleaseStatuses {
    param([string]$Commit)
    foreach ($path in $script:IndexedNormativePaths) {
        $text = Get-GitBlobText $Commit $path
        Assert-Condition $text.Contains('**Status:** Normative Release Final') "indexed normative document is not release-final: $path"
        Assert-Condition (-not $text.Contains('**Status:** Normative Implementation Candidate')) "candidate status remains in $path"
    }
    foreach ($path in $script:ArchitecturePaths) {
        $text = Get-GitBlobText $Commit $path
        Assert-Condition $text.Contains('**Status:** Internal Release Approved') "architecture document is not release approved: $path"
        Assert-Condition (-not $text.Contains('**Status:** Internal Implementation Candidate')) "candidate architecture status remains in $path"
    }
}

function Resolve-Phase14EvidenceCommit {
    param([string]$Value, [string]$EvidenceState)
    if ($Value -eq $script:Phase14EvidenceToken) { return $EvidenceState }
    return $Value
}

function Assert-Phase14Coverage {
    param([object[]]$Coverage, [object[]]$Requirements)
    Assert-Condition ($Coverage.Count -eq 597) "coverage registry contains $($Coverage.Count) rows rather than 597"
    $requirementsById = @{}
    foreach ($row in $Requirements) { $requirementsById[$row.requirement_id] = $row }
    $active = @($Coverage | Where-Object status -ne "superseded")
    $superseded = @($Coverage | Where-Object status -eq "superseded")
    Assert-Condition ($active.Count -eq 581) "active coverage count is $($active.Count) rather than 581"
    Assert-Condition ($superseded.Count -eq 16) "superseded coverage count is $($superseded.Count) rather than 16"
    foreach ($row in $active) {
        Assert-Condition ($row.status -eq "implemented") "active coverage remains planned: $($row.requirement_id)"
        Assert-Condition $requirementsById.ContainsKey($row.requirement_id) "coverage lacks a release owner: $($row.requirement_id)"
        $owner = $requirementsById[$row.requirement_id]
        Assert-Condition ($row.planned_evidence_id -eq $owner.evidence_id) "coverage evidence owner is stale for $($row.requirement_id)"
        Assert-Condition ($row.notes.Contains($owner.evidence_id)) "coverage notes omit the final evidence owner for $($row.requirement_id)"
        Assert-Condition ($row.notes -notmatch '[0-9a-f]{40}') "active coverage embeds a stale source identity: $($row.requirement_id)"
    }
    foreach ($row in $superseded) {
        Assert-Condition ($script:SupersededRequirementIds -contains $row.requirement_id) "unknown superseded coverage row: $($row.requirement_id)"
        Assert-Condition ($row.notes -match 'change record|supersed') "superseded coverage lacks change-record lineage: $($row.requirement_id)"
    }
}

function Assert-Phase14RequirementsAndClaims {
    param(
        [object[]]$Definitions,
        [object[]]$Evidence,
        [object[]]$RequirementRows,
        [object[]]$ClaimRows,
        [string]$Source,
        [string]$EvidenceState
    )
    Assert-Condition ($RequirementRows.Count -eq 581) "release requirement registry contains $($RequirementRows.Count) rows rather than 581"
    $definitionsById = @{}
    foreach ($definition in $Definitions) { $definitionsById[$definition.requirement_id] = $definition }
    $evidenceById = @{}
    foreach ($row in $Evidence) { $evidenceById[$row.evidence_id] = $row }
    $requirementsById = @{}
    foreach ($row in $RequirementRows) {
        Assert-Condition (-not $requirementsById.ContainsKey($row.requirement_id)) "duplicate release requirement $($row.requirement_id)"
        Assert-Condition $definitionsById.ContainsKey($row.requirement_id) "unknown or superseded release requirement $($row.requirement_id)"
        Assert-RequirementMetadata $row $definitionsById[$row.requirement_id]
        Assert-Condition $evidenceById.ContainsKey($row.evidence_id) "unknown evidence owner $($row.evidence_id)"
        $owner = $evidenceById[$row.evidence_id]
        Assert-Condition ($owner.requirement_id -eq $row.requirement_id) "evidence requirement mismatch for $($row.requirement_id)"
        Assert-Condition ($owner.result -eq "passing" -and $owner.configuration -match '^direct ') "release owner is not direct passing: $($row.evidence_id)"
        Assert-Condition ($row.result -eq "passing") "release requirement did not pass: $($row.requirement_id)"
        Assert-Condition ([string]::IsNullOrWhiteSpace($row.waiver_id)) "release requirement has an unapproved waiver: $($row.requirement_id)"
        $verified = Resolve-Phase14EvidenceCommit $owner.verified_commit $EvidenceState
        if ($row.evidence_commit_class -eq "evidence_state") {
            Assert-Condition ($verified -eq $EvidenceState) "evidence-state owner uses the wrong commit: $($row.evidence_id)"
            Assert-Condition ($row.current_direct_passing_evidence_owner -eq "Phase 14 release-final evidence state") "wrong evidence-state owner label: $($row.requirement_id)"
        } elseif ($row.evidence_commit_class -eq "immutable_source") {
            Assert-ExactCommit $verified "release evidence"
            Assert-Ancestor $verified $Source "immutable release evidence"
            Assert-Condition ($verified -ne $EvidenceState) "source evidence is classified as evidence state: $($row.evidence_id)"
            Assert-Condition ($row.current_direct_passing_evidence_owner -eq "Immutable validated source evidence") "wrong immutable-source owner label: $($row.requirement_id)"
        } else {
            throw "unsupported release evidence class in $($row.requirement_id): $($row.evidence_commit_class)"
        }
        $requirementsById[$row.requirement_id] = $row
    }
    foreach ($definition in $Definitions) {
        Assert-Condition $requirementsById.ContainsKey($definition.requirement_id) "active requirement lacks a release row: $($definition.requirement_id)"
    }

    $expectedClaims = @(Get-ExpectedClaims $Definitions)
    Assert-Condition ($expectedClaims.Count -eq $script:Phase14ClaimCount) "normative parser produced $($expectedClaims.Count) claims rather than $($script:Phase14ClaimCount)"
    Assert-Condition ($ClaimRows.Count -eq $script:Phase14ClaimCount) "release claim registry contains $($ClaimRows.Count) rows rather than $($script:Phase14ClaimCount)"
    $expectedById = @{}
    foreach ($claim in $expectedClaims) { $expectedById[$claim.claim_id] = $claim }
    $claimIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $requirementsWithClaims = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($claim in $ClaimRows) {
        Assert-Condition $claimIds.Add($claim.claim_id) "duplicate release claim $($claim.claim_id)"
        Assert-Condition $expectedById.ContainsKey($claim.claim_id) "unknown release claim $($claim.claim_id)"
        $expected = $expectedById[$claim.claim_id]
        Assert-Condition ($claim.requirement_id -eq $expected.requirement_id) "claim requirement mismatch: $($claim.claim_id)"
        Assert-Condition ($claim.claim_atom -eq $expected.claim_atom) "claim atom mismatch: $($claim.claim_id)"
        Assert-Condition $requirementsById.ContainsKey($claim.requirement_id) "claim has no release requirement: $($claim.claim_id)"
        $requirement = $requirementsById[$claim.requirement_id]
        $definition = $definitionsById[$claim.requirement_id]
        Assert-Condition ($claim.normative_document -eq $definition.normative_document) "claim document mismatch: $($claim.claim_id)"
        Assert-Condition ($claim.normative_title -eq $definition.normative_title) "claim title mismatch: $($claim.claim_id)"
        Assert-Condition ($claim.normative_paragraph_digest -eq $definition.normative_paragraph_digest) "claim digest mismatch: $($claim.claim_id)"
        Assert-Condition ($claim.evidence_id -eq $requirement.evidence_id) "claim evidence mismatch: $($claim.claim_id)"
        $owner = $evidenceById[$claim.evidence_id]
        Assert-Condition ($claim.classification -eq "direct") "claim is not direct: $($claim.claim_id)"
        Assert-Condition ($claim.proof_path -eq $owner.exact_path) "claim proof path mismatch: $($claim.claim_id)"
        Assert-Condition ($claim.proof_command -eq $owner.exact_test) "claim proof command mismatch: $($claim.claim_id)"
        Assert-Condition ((Resolve-Phase14EvidenceCommit $claim.proof_commit $EvidenceState) -eq (Resolve-Phase14EvidenceCommit $owner.verified_commit $EvidenceState)) "claim proof commit mismatch: $($claim.claim_id)"
        Assert-Condition ($claim.proof_command -ne "just release") "claim uses only the broad release aggregate: $($claim.claim_id)"
        Assert-CompleteClaim $claim
        [void]$requirementsWithClaims.Add($claim.requirement_id)
    }
    Assert-Condition ($requirementsWithClaims.Count -eq 581) "one or more active requirements lack a claim atom"
}

function Assert-Phase14ValidationRegistry {
    param([object[]]$Rows, [string]$Source)
    Assert-Condition ($Rows.Count -gt $script:Phase14HistoricalValidationRows) "Phase 14 appended no validation records"
    $ids = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $commandSources = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $phase14 = @($Rows | Where-Object validation_id -like "P14-VAL-*")
    Assert-Condition ($phase14.Count -eq ($Rows.Count - $script:Phase14HistoricalValidationRows)) "validation registry contains non-Phase-14 rows after the immutable prefix"
    foreach ($row in $Rows) {
        Assert-Condition $ids.Add($row.validation_id) "duplicate validation ID $($row.validation_id)"
        Assert-Condition $commandSources.Add("$($row.exact_command)|$($row.source_commit)") "duplicate validation command/source: $($row.exact_command)"
    }
    foreach ($row in $phase14) {
        Assert-Condition ($row.working_directory -eq "F:\projects\concord_api") "wrong Phase 14 validation directory: $($row.validation_id)"
        Assert-Condition ($row.source_commit -eq $Source) "Phase 14 validation is bound to the wrong source: $($row.validation_id)"
        Assert-Condition ($row.exit_status -eq "0" -and $row.result -eq "passing") "Phase 14 validation did not pass: $($row.validation_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.tool_versions)) "Phase 14 validation omits tool versions: $($row.validation_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.feature_configuration)) "Phase 14 validation omits feature configuration: $($row.validation_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.selected_tests)) "Phase 14 validation omits selected tests: $($row.validation_id)"
        Assert-Condition ($row.selected_tests -notmatch '(?:^|\D)0\s*/\s*0(?:\D|$)') "zero-test validation record: $($row.validation_id)"
        Assert-Condition ($row.observed_at -match '^2026-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$') "Phase 14 validation timestamp is not exact UTC: $($row.validation_id)"
    }
    $commands = [System.Collections.Generic.HashSet[string]]::new([string[]]$phase14.exact_command, [System.StringComparer]::Ordinal)
    $required = @(
        "pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode SelfTest",
        "powershell.exe -NoLogo -NoProfile -File scripts/audit-release-registry.ps1 -Mode SelfTest",
        "just _test-dangerous-features", "just _feature-matrix", "just _generic-spec-record-contracts",
        "just _record-feature-topology", "just _examples", "just _benchmarks", "just _msrv",
        "just _grammar-and-diagnostics", "just _reqwest-assumptions", "just _security-and-properties",
        "just _rustdoc-audits", "just _release-audits",
        "just fmt-check", "just check", "just check-all", "just check-no-default", "just clippy", "just clippy-all",
        "just test", "just test-all", "just test-no-default", "just doctest", "just doctest-all",
        "just doc", "just doc-all", "just deny", "just diff-check", "just release"
    )
    foreach ($command in $required) {
        Assert-Condition $commands.Contains($command) "required Phase 14 validation command is unrecorded: $command"
    }
    $sourceAudit = @($phase14 | Where-Object exact_command -match 'audit-release-registry\.ps1 -Mode Source .* -PhaseBaseline 59e888a352b6bb612972066c07596b5ac28ad36b')
    Assert-Condition ($sourceAudit.Count -ge 1) "explicit Phase 14 source-mode audit is unrecorded"
    $integration = @($phase14 | Where-Object exact_command -match 'paginated_tls_recovery_reuses_lexical_plan_across_recovery_and_later_page')
    Assert-Condition ($integration.Count -ge 1) "combined authentication/pagination/rate-plan proof is unrecorded"
    return $commands
}

function Assert-Phase14EvidenceCommands {
    param([object[]]$RequirementRows, [object[]]$Evidence, [object[]]$Validation, [string]$EvidenceState)
    $evidenceById = @{}
    foreach ($row in $Evidence) { $evidenceById[$row.evidence_id] = $row }
    $commands = [System.Collections.Generic.HashSet[string]]::new([string[]]$Validation.exact_command, [System.StringComparer]::Ordinal)
    foreach ($requirement in $RequirementRows) {
        $owner = $evidenceById[$requirement.evidence_id]
        $verified = Resolve-Phase14EvidenceCommit $owner.verified_commit $EvidenceState
        if ($owner.evidence_id -like "P14-*" -or $verified -eq $EvidenceState) {
            Assert-Condition $commands.Contains($owner.exact_test) "Phase 14 release evidence command is absent from validation: $($owner.evidence_id)"
        }
    }
}

function Assert-Phase14ChangeMap {
    param([object[]]$Rows, [object[]]$Definitions, [object[]]$Validation, [string]$Source, [string]$EvidenceState, [bool]$RequireProofCommands = $true)
    $expectedPaths = @(& git diff --name-only "$($script:Phase14HistoricalReleaseSource)..$EvidenceState")
    Assert-Condition ($LASTEXITCODE -eq 0) "unable to derive the final release change surface"
    $sourcePaths = [System.Collections.Generic.HashSet[string]]::new(
        [string[]]@(& git diff --name-only "$($script:Phase14HistoricalReleaseSource)..$Source"),
        [System.StringComparer]::Ordinal
    )
    $evidencePaths = [System.Collections.Generic.HashSet[string]]::new(
        [string[]]@(& git diff --name-only "$Source..$EvidenceState"),
        [System.StringComparer]::Ordinal
    )
    $known = [System.Collections.Generic.HashSet[string]]::new([string[]]$Definitions.requirement_id, [System.StringComparer]::Ordinal)
    $commands = [System.Collections.Generic.HashSet[string]]::new([string[]]$Validation.exact_command, [System.StringComparer]::Ordinal)
    $ids = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $paths = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Rows) {
        Assert-Condition $ids.Add($row.change_id) "duplicate release change ID $($row.change_id)"
        Assert-Condition $paths.Add($row.exact_path) "duplicate release change path $($row.exact_path)"
        Assert-DurablePath $row.exact_path $row.change_id
        Assert-Condition (-not $row.exact_path.StartsWith("git_diff/")) "ignored path entered release change map: $($row.exact_path)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.observable_change)) "release change lacks an observable description: $($row.change_id)"
        Assert-Condition ($row.result -eq "passing") "release change did not pass: $($row.change_id)"
        $expectedClass = if ($evidencePaths.Contains($row.exact_path)) { "evidence_state" } else { "immutable_source" }
        Assert-Condition ($sourcePaths.Contains($row.exact_path) -or $evidencePaths.Contains($row.exact_path)) "release change path is absent from Git range: $($row.exact_path)"
        Assert-Condition ($row.proof_commit_class -eq $expectedClass) "release change has the wrong commit class: $($row.exact_path)"
        foreach ($requirementId in @($row.requirement_ids -split ';')) {
            Assert-Condition $known.Contains($requirementId) "release change maps to unknown or obsolete requirement $requirementId"
        }
        if ($RequireProofCommands) {
            Assert-Condition $commands.Contains($row.proof_command) "release change proof command is unregistered: $($row.change_id)"
        }
    }
    Assert-Condition ($paths.Count -eq $expectedPaths.Count) "release change-map count mismatch: $($paths.Count) vs $($expectedPaths.Count)"
    foreach ($path in $expectedPaths) { Assert-Condition $paths.Contains($path) "unmapped release change path: $path" }
}

function Assert-Phase14ReleaseDocumentation {
    param([string]$Commit)
    $readme = Get-GitBlobText $Commit "README.md"
    $docsReadme = Get-GitBlobText $Commit "docs/README.md"
    $report = Get-GitBlobText $Commit $script:Phase14ReportPath
    $program = Get-GitBlobText $Commit "docs/planning/CONCORD_IMPLEMENTATION_PROGRAM_0.2.md"
    $index = Get-GitBlobText $Commit "docs/normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md"
    $combined = "$readme`n$docsReadme`n$report`n$program`n$index"
    foreach ($required in @("PHASE_14_COMPLETE", "Concord 0.1 Conforming Release", "Normative Release Final")) {
        Assert-Condition $combined.Contains($required) "release documentation omits $required"
    }
    Assert-Condition ($combined -notmatch 'Phase 14 (?:is )?not started|PHASE_14_NOT_STARTED|current package is a `?Normative Implementation Candidate') "transitional Phase 14 wording remains"
    Assert-Condition ($combined -notmatch '(?i)Phase 15|published to crates\.io|deployed release|tagged release') "release documentation claims Phase 15 or an unauthorized external release operation"
    Assert-Condition ($combined -match '(?i)not published|no publication') "release documentation omits the local-only publication boundary"
}

function Assert-Phase14Controller {
    param(
        [string]$Commit,
        [string]$Source,
        [string]$EvidenceState,
        [string]$Anchor,
        [int]$EvidenceRows,
        [int]$ChangeRows,
        [int]$ValidationRows
    )
    $controller = Get-GitBlobText $Commit $script:ControllerPath
    $controller = $controller.Replace($script:Phase14EvidenceToken, $EvidenceState)
    if (-not [string]::IsNullOrWhiteSpace($Anchor)) {
        $controller = $controller.Replace($script:Phase14AnchorToken, $Anchor)
    }
    foreach ($required in @(
        'latest_validated_phase = "14"', 'latest_verdict = "PHASE_14_COMPLETE"',
        "source_commit = `"$Source`"", "evidence_commit = `"$EvidenceState`"",
        'normative_status = "Normative Release Final"', 'head_policy = "anchor-must-be-an-ancestor"',
        'claim_atom_rows = 979', 'proved_claim_atoms = 979', 'unresolved_claim_atoms = 0',
        'release_requirement_registry_rows = 581', 'release_claim_registry_rows = 979',
        "catalog_evidence_rows = $EvidenceRows", "release_change_map_rows = $ChangeRows",
        "release_validation_rows = $ValidationRows", 'id = "14"', 'status = "COMPLETE"'
    )) {
        Assert-Condition $controller.Contains($required) "controller release state mismatch: $required"
    }
    if (-not [string]::IsNullOrWhiteSpace($Anchor)) {
        Assert-Condition $controller.Contains("anchor_commit = `"$Anchor`"") "controller does not record the actual Phase 14 anchor"
    }
    Assert-Condition ($controller -notmatch '(?m)^\s*id\s*=\s*"15"') "controller contains a Phase 15 entry"
}

function Assert-Phase14AnchorPaths {
    param([string]$EvidenceState, [string]$Anchor)
    $allowed = @(
        $script:EvidencePath,
        $script:ClaimRegistryPath,
        $script:ValidationRegistryPath,
        $script:Phase14ReportPath,
        $script:ControllerPath
    )
    $changed = @(& git diff --name-only "$EvidenceState..$Anchor")
    Assert-Condition ($LASTEXITCODE -eq 0 -and $changed.Count -gt 0) "Phase 14 anchor has no documentation changes"
    foreach ($path in $changed) {
        Assert-Condition ($allowed -contains $path) "Phase 14 anchor changes an unapproved path: $path"
    }
    $executable = @(& git diff --name-only "$EvidenceState..$Anchor" -- ':!docs/**')
    Assert-Condition ($executable.Count -eq 0) "Phase 14 anchor changes executable state"
}

function Invoke-Phase14SelfTests {
    $head = (& git rev-parse HEAD).Trim()
    Assert-ExactCommit $head "self-test HEAD"
    Assert-Condition ($script:IndexedNormativePaths.Count -eq 12) "self-test indexed normative set is not 12 documents"
    Assert-Condition ($script:ArchitecturePaths.Count -eq 3) "self-test architecture set is not three documents"
    $documents = Get-NormativeDocuments $head
    $definitions = @(Get-NormativeDefinitions $documents)
    $claims = @(Get-ExpectedClaims $definitions)
    Assert-Condition ($definitions.Count -eq 581) "self-test parser found $($definitions.Count) requirements"
    Assert-Condition ($claims.Count -eq 979) "self-test parser found $($claims.Count) claims"
    foreach ($accepted in @(
        "scripts/audit-release-registry.ps1", "justfile", ".config/nextest.toml",
        "concord_core/src/execute/auth_tests.rs", "concord_macros/tests/ui.rs",
        "concord_examples/examples/authentication.rs", "perf/benches/concord_overhead.rs", "Cargo.lock"
    )) {
        Assert-Condition (Test-Phase14SourcePath $accepted) "self-test rejected executable source path $accepted"
    }
    foreach ($rejected in @(
        $script:EvidencePath, $script:CoveragePath, $script:RequirementRegistryPath,
        $script:ClaimRegistryPath, $script:ChangeMapPath, $script:ValidationRegistryPath,
        $script:Phase14ReportPath, $script:ControllerPath, "README.md",
        "docs/normative/00_CONCORD_SPECIFICATION_INDEX_0.1.md", "git_diff/ignored.md"
    )) {
        Assert-Condition (-not (Test-Phase14SourcePath $rejected)) "self-test accepted release state in source range: $rejected"
    }
    Assert-Phase14SourcePaths @("scripts/audit-release-registry.ps1", "concord_core/src/execute/auth_tests.rs")
    Assert-Throws { Assert-Phase14SourcePaths @("scripts/audit-release-registry.ps1", $script:EvidencePath) } "evidence in source range"
    Assert-Phase14PostSourcePaths @(
        $script:EvidencePath,
        $script:Phase14ReportPath,
        $script:IndexedNormativePaths[0],
        $script:ArchitecturePaths[0],
        "docs/REVIEW_RESOLUTION_0.1.md",
        "docs/decisions/CONCORD_DECISIONS.md"
    )
    Assert-Throws { Assert-Phase14PostSourcePaths @($script:Phase14ReportPath, "concord_core/src/lib.rs") } "executable post-source change"
    Assert-Throws { Assert-DurablePath "git_diff/ignored.md" "P14-SELFTEST" } "ignored evidence path"
    Assert-Throws { Assert-Condition $false "missing commit-scoped path" } "missing path"
    $alwaysAncestor = { param($left, $right) return $true }
    $neverAncestor = { param($left, $right) return $false }
    Assert-Phase14BaselineInput $script:Phase14Baseline "1111111111111111111111111111111111111111" $alwaysAncestor
    Assert-Throws { Assert-Phase14BaselineInput "" "source" $alwaysAncestor } "missing Phase 14 baseline"
    Assert-Throws { Assert-Phase14BaselineInput "1111111111111111111111111111111111111111" "source" $alwaysAncestor } "wrong Phase 14 baseline"
    Assert-Throws { Assert-Phase14BaselineInput $script:Phase14Baseline $script:Phase14Baseline $alwaysAncestor } "baseline/source equality"
    Assert-Throws { Assert-Phase14BaselineInput $script:Phase14Baseline "source" $neverAncestor } "non-ancestral Phase 14 baseline"
    $prefix = [byte[]](1, 2, 3)
    Assert-BytePrefix $prefix ([byte[]](1, 2, 3, 4)) "self-test prefix"
    Assert-Throws { Assert-BytePrefix $prefix ([byte[]](1, 9, 3)) "self-test mutation" } "historical prefix mutation"
    $definition = [pscustomobject]@{ requirement_id = "TEST-REQ-001"; normative_document = "doc"; normative_title = "Title"; normative_paragraph_digest = "digest" }
    $row = [pscustomobject]@{ requirement_id = "TEST-REQ-001"; normative_document = "doc"; normative_title = "Wrong"; normative_paragraph_digest = "digest" }
    Assert-Throws { Assert-RequirementMetadata $row $definition } "title drift"
    $row.normative_title = "Title"; $row.normative_paragraph_digest = "wrong"
    Assert-Throws { Assert-RequirementMetadata $row $definition } "digest drift"
    $known = [System.Collections.Generic.HashSet[string]]::new([string[]]@("TEST-REQ-001"), [System.StringComparer]::Ordinal)
    Assert-Throws { Assert-KnownRequirement "UNKNOWN-001" $known "self-test" } "unknown requirement"
    Assert-Throws { Assert-NoWaiver "waived" "W-1" "self-test" } "unapproved waiver"
    $untracked = [pscustomobject]@{ normative_document = "doc"; normalized_statement = "Behavior MUST drift."; normalized_statement_digest = "x" }
    Assert-Throws { Assert-NormativeLanguage @($untracked) @() } "untracked normative prose"
    $cycle = @(
        [pscustomobject]@{ dependent_document = "A"; authority_document = "B" },
        [pscustomobject]@{ dependent_document = "B"; authority_document = "A" }
    )
    Assert-Throws { Assert-AcyclicDependencies $cycle } "normative dependency cycle"
    $duplicates = @(
        [pscustomobject]@{ subject = "public_signatures"; authoritative_document = "03_CONCORD_PUBLIC_API_SPEC_0.1.md"; requirement_prefixes = "API-"; audit_rule = "unique-prefix-owner-and-language-audit"; result = "passing" },
        [pscustomobject]@{ subject = "public_signatures"; authoritative_document = "02_CONCORD_DSL_SPEC_0.1.md"; requirement_prefixes = "DSL-"; audit_rule = "unique-prefix-owner-and-language-audit"; result = "passing" }
    )
    Assert-Throws { Assert-OwnershipRegistry $duplicates @() } "duplicate normative owner"
    $incomplete = [pscustomobject]@{ claim_id = "X"; requirement_id = "TEST-REQ-001"; claim_atom = ""; result = "passing"; later_owner = "later" }
    Assert-Throws { Assert-CompleteClaim $incomplete } "incomplete claim"
    Assert-Throws { Assert-Phase14SourcePaths @() } "empty source boundary"
    Write-Output "release registry audit self-tests passed: 30 rejection/acceptance classes; active-requirements=581; parsed-claims=979"
}

if ($Mode -eq "SelfTest") {
    Invoke-Phase14SelfTests
    exit 0
}

Assert-Condition (-not [string]::IsNullOrWhiteSpace($SourceCommit)) "Phase 14 audit requires SourceCommit"
Assert-ExactCommit $SourceCommit "SourceCommit"
Assert-Phase14Boundary $PhaseBaseline $SourceCommit

if ($Mode -eq "Source") {
    $head = (& git rev-parse HEAD).Trim()
    Assert-Condition ($head -eq $SourceCommit) "source mode must run at the exact SourceCommit"
    $documents = Get-NormativeDocuments $SourceCommit
    $definitions = @(Get-NormativeDefinitions $documents)
    $coverage = @(Import-GitCsv $SourceCommit $script:CoveragePath)
    $evidence = @(Import-GitCsv $SourceCommit $script:EvidencePath)
    $validation = @(Import-GitCsv $SourceCommit $script:ValidationRegistryPath)
    Assert-BaseCatalogs $definitions $coverage $evidence
    Assert-Condition ($definitions.Count -eq 581) "source parser did not find 581 active requirements"
    Assert-Condition ((Get-ExpectedClaims $definitions).Count -eq 979) "source parser did not find 979 claim atoms"
    Assert-Condition ($evidence.Count -eq $script:Phase14HistoricalEvidenceRows) "source commit changed the inherited evidence catalog"
    Assert-Condition ($validation.Count -eq $script:Phase14HistoricalValidationRows) "source commit changed the inherited validation registry"
    Assert-Phase14HistoricalPrefixes $SourceCommit
    Assert-Phase14ReportsPreserved $SourceCommit
    Assert-Phase14NormativeParagraphsPreserved $definitions
    Assert-Phase14CandidateState $SourceCommit
    Assert-EvidencePaths $evidence $SourceCommit
    Write-Output "source validation passed: phase-baseline=$PhaseBaseline source=$SourceCommit active-requirements=581 claim-atoms=979 historical-evidence-rows=1726"
    Write-Output "release-final evidence completeness not assessed"
    exit 0
}

Assert-Condition (-not [string]::IsNullOrWhiteSpace($EvidenceCommit)) "$Mode mode requires EvidenceCommit"
Assert-ExactCommit $EvidenceCommit "EvidenceCommit"
Assert-Ancestor $SourceCommit $EvidenceCommit "source/evidence chain"

if ($Mode -eq "Evidence") {
    $head = (& git rev-parse HEAD).Trim()
    Assert-Condition ($head -eq $EvidenceCommit) "evidence mode must run at the exact EvidenceCommit"
    $StateCommit = $EvidenceCommit
    $DescendantCommit = $EvidenceCommit
    $ActualAnchor = ""
    $RequireRegisteredEvidenceAudit = $false
} else {
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($AnchorCommit)) "final mode requires AnchorCommit"
    Assert-ExactCommit $AnchorCommit "AnchorCommit"
    Assert-Ancestor $EvidenceCommit $AnchorCommit "evidence/anchor chain"
    Assert-Phase14AnchorPaths $EvidenceCommit $AnchorCommit
    $ActualAnchor = $AnchorCommit
    $RequireRegisteredEvidenceAudit = $true
    if ([string]::IsNullOrWhiteSpace($MetadataCommit)) {
        $head = (& git rev-parse HEAD).Trim()
        Assert-Condition ($head -eq $AnchorCommit) "anchor-final mode must run at the exact AnchorCommit"
        $StateCommit = $AnchorCommit
        $DescendantCommit = $AnchorCommit
    } else {
        Assert-ExactCommit $MetadataCommit "MetadataCommit"
        Assert-Ancestor $AnchorCommit $MetadataCommit "anchor/metadata chain"
        $metadataPaths = @(& git diff --name-only "$AnchorCommit..$MetadataCommit")
        Assert-Condition ($LASTEXITCODE -eq 0) "metadata descendant path inspection failed"
        foreach ($path in $metadataPaths) {
            Assert-Condition ($path -in @($script:Phase14ReportPath, $script:ControllerPath)) "metadata descendant changes an unapproved path: $path"
        }
        $head = (& git rev-parse HEAD).Trim()
        Assert-Condition ($head -eq $MetadataCommit) "final mode must run at the exact MetadataCommit"
        $StateCommit = $MetadataCommit
        $DescendantCommit = $MetadataCommit
    }
}

$postSourcePaths = @(& git diff --name-only "$SourceCommit..$DescendantCommit")
Assert-Condition ($LASTEXITCODE -eq 0) "post-source path inspection failed"
Assert-Phase14PostSourcePaths $postSourcePaths

$documents = Get-NormativeDocuments $StateCommit
$definitions = @(Get-NormativeDefinitions $documents)
$coverage = @(Import-GitCsv $StateCommit $script:CoveragePath)
$evidence = @(Import-GitCsv $StateCommit $script:EvidencePath)
foreach ($row in $evidence) {
    $row.verified_commit = Resolve-Phase14EvidenceCommit $row.verified_commit $EvidenceCommit
}
$requirements = @(Import-GitCsv $StateCommit $script:RequirementRegistryPath)
$claims = @(Import-GitCsv $StateCommit $script:ClaimRegistryPath)
foreach ($row in $claims) {
    $row.proof_commit = Resolve-Phase14EvidenceCommit $row.proof_commit $EvidenceCommit
}
$changes = @(Import-GitCsv $StateCommit $script:ChangeMapPath)
$validation = @(Import-GitCsv $StateCommit $script:ValidationRegistryPath)
$allowlist = @(Import-GitCsv $StateCommit $script:LanguageAllowlistPath)
$dependencies = @(Import-GitCsv $StateCommit $script:DependencyRegistryPath)
$ownership = @(Import-GitCsv $StateCommit $script:OwnershipRegistryPath)

Assert-BaseCatalogs $definitions $coverage $evidence
Assert-Phase14HistoricalPrefixes $StateCommit
Assert-Phase14ReportsPreserved $StateCommit
Assert-Phase14NormativeParagraphsPreserved $definitions
Assert-Phase14ReleaseStatuses $StateCommit
Assert-Phase14RequirementsAndClaims $definitions $evidence $requirements $claims $SourceCommit $EvidenceCommit
Assert-Phase14Coverage $coverage $requirements
foreach ($row in $evidence) {
    Assert-Condition ($row.result -eq "passing" -and [string]::IsNullOrWhiteSpace($row.waiver)) "nonpassing or waived release evidence: $($row.evidence_id)"
}
$phase14Evidence = @($evidence | Where-Object evidence_id -like "P14-*")
Assert-Condition ($phase14Evidence.Count -gt 0) "Phase 14 appended no evidence rows"
foreach ($row in $phase14Evidence) {
    Assert-Condition ($row.configuration -match '^direct ' -and $row.configuration -notmatch '(?i)partial') "Phase 14 evidence is not wholly direct: $($row.evidence_id)"
}
Assert-EvidencePaths $evidence $DescendantCommit
$outsideStatements = @(Get-OutsideNormativeStatements $documents $definitions)
Assert-NormativeLanguage $outsideStatements $allowlist
Assert-DependencyRegistry $dependencies $definitions
Assert-OwnershipRegistry $ownership $definitions
$phase14Commands = Assert-Phase14ValidationRegistry $validation $SourceCommit
Assert-Phase14ChangeMap $changes $definitions $validation $SourceCommit $EvidenceCommit $RequireRegisteredEvidenceAudit
if ($RequireRegisteredEvidenceAudit) {
    Assert-Phase14EvidenceCommands $requirements $evidence $validation $EvidenceCommit
}
Assert-Phase14ReleaseDocumentation $StateCommit
Assert-Phase14Controller $StateCommit $SourceCommit $EvidenceCommit $ActualAnchor $evidence.Count $changes.Count $validation.Count

if ($Mode -eq "Final") {
    $status = @(& git status --porcelain)
    Assert-Condition ($status.Count -eq 0) "final worktree is not clean"
}

Write-Output "release-final audit passed: mode=$Mode active-requirements=581 superseded-requirements=16 claims=979 evidence=$($evidence.Count) Phase-14-evidence=$($phase14Evidence.Count) change-map=$($changes.Count) validation=$($validation.Count) waivers=0 unresolved-claims=0 missing-paths=0"
exit 0

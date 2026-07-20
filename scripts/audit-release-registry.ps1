[CmdletBinding()]
param(
    [ValidateSet("SelfTest", "Source", "Evidence", "Final")]
    [string]$Mode = "Source",
    [string]$SourceCommit = "",
    [string]$EvidenceCommit = "",
    [string]$AnchorCommit = "",
    [string]$MetadataCommit = "",
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
    Assert-Condition ($Definitions.Count -eq 504) "normative owner set contains $($Definitions.Count) definitions rather than 504"
    Assert-Condition ($Coverage.Count -eq 504) "coverage registry contains $($Coverage.Count) rows rather than 504"
    $definitionIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($definition in $Definitions) {
        Assert-Condition $definitionIds.Add($definition.requirement_id) "duplicate normative owner $($definition.requirement_id)"
    }
    $coverageIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Coverage) {
        Assert-Condition $coverageIds.Add($row.requirement_id) "duplicate coverage requirement $($row.requirement_id)"
        Assert-KnownRequirement $row.requirement_id $definitionIds "coverage row"
    }
    foreach ($id in $definitionIds) {
        Assert-Condition $coverageIds.Contains($id) "normative ID is absent from coverage: $id"
    }

    $evidenceIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($row in $Evidence) {
        Assert-KnownRequirement $row.requirement_id $definitionIds "evidence row $($row.evidence_id)"
        Assert-Condition $evidenceIds.Add($row.evidence_id) "duplicate evidence ID $($row.evidence_id)"
        Assert-Condition $script:AllowedLabels.Contains($row.evidence_type) "unapproved evidence label $($row.evidence_type) in $($row.evidence_id)"
        Assert-Condition ($row.result -in @("passing", "waived")) "invalid result in $($row.evidence_id): $($row.result)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.exact_path)) "missing exact path in $($row.evidence_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.exact_test)) "missing exact command in $($row.evidence_id)"
        Assert-Condition (-not [string]::IsNullOrWhiteSpace($row.verified_commit)) "missing verified commit in $($row.evidence_id)"
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
        Assert-Condition $commands.Add($row.exact_command) "duplicate validation command $($row.exact_command)"
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
    Assert-Condition ($parsedDefinitions.Count -eq 504) "self-test normative parser did not find 504 requirements"
    Assert-Condition ($parsedClaims.Count -gt 504) "self-test claim parser did not decompose compound requirements"
    $fileType = (& git cat-file -t "$head`:Cargo.toml").Trim()
    $directoryType = (& git cat-file -t "$head`:concord_core/src").Trim()
    Assert-Condition ($fileType -eq "blob") "self-test did not accept a valid file"
    Assert-Condition ($directoryType -eq "tree") "self-test did not accept a valid directory"
    Assert-Throws { $type = @(& git cat-file -t "$head`:missing-release-audit-path" 2>$null); Assert-Condition ($LASTEXITCODE -eq 0) "missing" } "missing path"
    Assert-Throws { Assert-DurablePath "git_diff/ignored.md" "SELFTEST" } "ignored git_diff path"

    $alwaysAncestor = { param($left, $right) return $true }
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
    $neverAncestor = { param($left, $right) return $false }
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
    $expectedRequirements = [System.Collections.Generic.HashSet[string]]::new([string[]]@("A", "B"), [System.StringComparer]::Ordinal)
    $resolvedRequirements = [System.Collections.Generic.HashSet[string]]::new([string[]]@("A"), [System.StringComparer]::Ordinal)
    Assert-Throws {
        foreach ($id in $expectedRequirements) { Assert-Condition $resolvedRequirements.Contains($id) "unresolved release requirement $id" }
    } "unresolved release requirement"
    Write-Output "release registry audit self-tests passed: 15 rejection/acceptance classes; parsed-claims=$($parsedClaims.Count)"
}

if ($Mode -eq "SelfTest") {
    Invoke-SelfTests
    exit 0
}

if ([string]::IsNullOrWhiteSpace($SourceCommit)) {
    $SourceCommit = (& git rev-parse HEAD).Trim()
}
Assert-ExactCommit $HistoricalBaseline "HistoricalBaseline"
Assert-ExactCommit $SourceCommit "SourceCommit"
Assert-Ancestor $HistoricalBaseline $SourceCommit "historical baseline"

if ($Mode -eq "Source") {
    $head = (& git rev-parse HEAD).Trim()
    Assert-Condition ($head -eq $SourceCommit) "source mode must run at the tested source commit"
    $documents = Get-NormativeDocuments $SourceCommit
    $definitions = @(Get-NormativeDefinitions $documents)
    $coverage = @(Import-GitCsv $SourceCommit $script:CoveragePath)
    $evidence = @(Import-GitCsv $SourceCommit $script:EvidencePath)
    Assert-BaseCatalogs $definitions $coverage $evidence
    $historicalRows = @($evidence | Where-Object { $_.evidence_id -notlike "P10-*" })
    Assert-Condition ($historicalRows.Count -eq 1473) "source mode historical row count changed: $($historicalRows.Count)"
    Assert-EvidencePaths $historicalRows $SourceCommit
    Assert-HistoricalPrefix $SourceCommit
    Write-Output "release registry source audit passed: requirements=504 historical-rows=1473 final-evidence-completeness=not-assessed"
    exit 0
}

Assert-ExactCommit $EvidenceCommit "EvidenceCommit"
Assert-Ancestor $SourceCommit $EvidenceCommit "source/evidence chain"
if ($Mode -eq "Evidence") {
    $StateCommit = $EvidenceCommit
    $DescendantCommit = $EvidenceCommit
} else {
    Assert-ExactCommit $AnchorCommit "AnchorCommit"
    Assert-ExactCommit $MetadataCommit "MetadataCommit"
    Assert-Ancestor $EvidenceCommit $AnchorCommit "evidence/anchor chain"
    Assert-Ancestor $AnchorCommit $MetadataCommit "anchor/metadata chain"
    $head = (& git rev-parse HEAD).Trim()
    Assert-Condition ($head -eq $MetadataCommit) "final mode must run at MetadataCommit"
    $StateCommit = $AnchorCommit
    $DescendantCommit = $MetadataCommit
}

$postSource = @(& git diff --name-only "$SourceCommit..$DescendantCommit")
Assert-Condition ($LASTEXITCODE -eq 0) "post-source path audit failed"
$invalidPostSource = @($postSource | Where-Object { $_ -ne "README.md" -and $_ -notlike "docs/*" })
Assert-Condition ($invalidPostSource.Count -eq 0) "post-source executable changes: $($invalidPostSource -join ', ')"

$documents = Get-NormativeDocuments $StateCommit
$definitions = @(Get-NormativeDefinitions $documents)
$coverage = @(Import-GitCsv $StateCommit $script:CoveragePath)
$evidence = @(Import-GitCsv $StateCommit $script:EvidencePath)
foreach ($row in $evidence) {
    $row.verified_commit = Resolve-EvidenceCommitValue $row.verified_commit $EvidenceCommit
}
Assert-BaseCatalogs $definitions $coverage $evidence
foreach ($row in $evidence) {
    Assert-Condition ($row.result -ne "waived" -and [string]::IsNullOrWhiteSpace($row.waiver)) "unapproved waiver in $($row.evidence_id)"
}
Assert-EvidencePaths $evidence $DescendantCommit
Assert-HistoricalPrefix $StateCommit

$requirements = @(Import-GitCsv $StateCommit $script:RequirementRegistryPath)
$claims = @(Import-GitCsv $StateCommit $script:ClaimRegistryPath)
foreach ($claim in $claims) { $claim.proof_commit = Resolve-EvidenceCommitValue $claim.proof_commit $EvidenceCommit }
$claimCount = Assert-RequirementAndClaimRegistries $definitions $evidence $requirements $claims $SourceCommit $EvidenceCommit $HistoricalBaseline

$allowlist = @(Import-GitCsv $StateCommit $script:LanguageAllowlistPath)
$outsideStatements = @(Get-OutsideNormativeStatements $documents $definitions)
Assert-NormativeLanguage $outsideStatements $allowlist
$dependencies = @(Import-GitCsv $StateCommit $script:DependencyRegistryPath)
Assert-DependencyRegistry $dependencies $definitions
$ownership = @(Import-GitCsv $StateCommit $script:OwnershipRegistryPath)
Assert-OwnershipRegistry $ownership $definitions
$changes = @(Import-GitCsv $StateCommit $script:ChangeMapPath)
Assert-ChangeMap $changes $definitions $SourceCommit $EvidenceCommit
$validation = @(Import-GitCsv $StateCommit $script:ValidationRegistryPath)
Assert-ValidationRegistry $validation $SourceCommit

foreach ($path in $script:NormativePaths) {
    Assert-Condition $documents[$path].Contains('**Status:** Normative Release Final') "normative document is not release-final: $path"
}
Assert-ReleaseDocumentation $StateCommit

if ($Mode -eq "Final") {
    $anchorAllowed = @(
        $script:EvidencePath, $script:RequirementRegistryPath, $script:ClaimRegistryPath,
        "docs/conformance/PHASE_10_REPORT.md", "docs/controller/PROJECT_CONTROL.toml",
        "docs/planning/CONCORD_IMPLEMENTATION_PROGRAM_0.2.md", "README.md", "docs/README.md"
    )
    Assert-SubstitutionCommit $EvidenceCommit $AnchorCommit $script:EvidenceToken $EvidenceCommit $anchorAllowed
    Assert-SubstitutionCommit $AnchorCommit $MetadataCommit $script:AnchorToken $AnchorCommit @(
        "docs/conformance/PHASE_10_REPORT.md", "docs/controller/PROJECT_CONTROL.toml"
    )
    $controller = Get-GitBlobText $MetadataCommit "docs/controller/PROJECT_CONTROL.toml"
    foreach ($required in @(
        'latest_validated_phase = "10"', 'latest_verdict = "PHASE_10_COMPLETE"',
        "source_commit = `"$SourceCommit`"", "evidence_commit = `"$EvidenceCommit`"",
        "anchor_commit = `"$AnchorCommit`"", 'head_policy = "anchor-must-be-an-ancestor"',
        'normative_status = "Normative Release Final"', 'unresolved_claim_atoms = 0',
        'status = "COMPLETE"'
    )) {
        Assert-Condition $controller.Contains($required) "controller state missing: $required"
    }
    $status = @(& git status --porcelain)
    Assert-Condition ($status.Count -eq 0) "final worktree is not clean"
}

Write-Output "release registry audit passed: mode=$Mode requirements=504 claims=$claimCount evidence=$($evidence.Count) waivers=0 missing-paths=0"

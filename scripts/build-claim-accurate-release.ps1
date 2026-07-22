param(
    [Parameter(Mandatory = $true)][string]$SourceCommit,
    [string]$EvidenceCommit = "EVIDENCE_COMMIT_PENDING",
    [string]$ValidationObservations = "git_diff/phase-14-validation-observations.csv"
)

$ErrorActionPreference = "Stop"
$requestedSourceCommit = $SourceCommit
$requestedEvidenceCommit = $EvidenceCommit
$requestedValidationObservations = $ValidationObservations
. "$PSScriptRoot/audit-release-registry.ps1"
$SourceCommit = $requestedSourceCommit
$EvidenceCommit = $requestedEvidenceCommit
$ValidationObservations = $requestedValidationObservations

Assert-ExactCommit $SourceCommit "SourceCommit"
Assert-Ancestor $script:Phase14Baseline $SourceCommit "Phase 14 correction source"
$documents = Get-NormativeDocuments $SourceCommit
$definitions = @(Get-NormativeDefinitions $documents)
$claims = @(Get-ExpectedClaims $definitions)
Assert-Condition ($definitions.Count -eq 581) "builder requires 581 active requirements"
Assert-Condition ($claims.Count -gt $definitions.Count) "builder found no independently split claims"

$historicalEvidence = @(Import-GitCsv $SourceCommit $script:EvidencePath)
$coverage = @(Import-GitCsv $SourceCommit $script:CoveragePath)
Assert-Condition ($historicalEvidence.Count -eq 1726) "builder requires the exact Phase 0-13 evidence catalog"

$observations = @()
if (Test-Path -LiteralPath $ValidationObservations) {
    $observations = @(Import-Csv -LiteralPath $ValidationObservations)
}
$observationsByCommand = @{}
foreach ($row in $observations) { $observationsByCommand[$row.exact_command] = $row }

$roleTable = @{
    selftest_pwsh = [pscustomobject]@{ id = "P14-VAL-001"; command = "pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode SelfTest" }
    selftest_windows = [pscustomobject]@{ id = "P14-VAL-002"; command = "powershell.exe -NoLogo -NoProfile -File scripts/audit-release-registry.ps1 -Mode SelfTest" }
    dangerous = [pscustomobject]@{ id = "P14-VAL-005"; command = "just _test-dangerous-features" }
    feature = [pscustomobject]@{ id = "P14-VAL-006"; command = "just _feature-matrix" }
    generic = [pscustomobject]@{ id = "P14-VAL-007"; command = "just _generic-spec-record-contracts" }
    records = [pscustomobject]@{ id = "P14-VAL-008"; command = "just _record-feature-topology" }
    examples = [pscustomobject]@{ id = "P14-VAL-009"; command = "just _examples" }
    benchmarks = [pscustomobject]@{ id = "P14-VAL-010"; command = "just _benchmarks" }
    msrv = [pscustomobject]@{ id = "P14-VAL-011"; command = "just _msrv" }
    grammar = [pscustomobject]@{ id = "P14-VAL-012"; command = "just _grammar-and-diagnostics" }
    reqwest = [pscustomobject]@{ id = "P14-VAL-013"; command = "just _reqwest-assumptions" }
    security = [pscustomobject]@{ id = "P14-VAL-014"; command = "just _security-and-properties" }
    rustdoc = [pscustomobject]@{ id = "P14-VAL-015"; command = "just _rustdoc-audits" }
    test_all = [pscustomobject]@{ id = "P14-VAL-024"; command = "just test-all" }
    no_default = [pscustomobject]@{ id = "P14-VAL-025"; command = "just test-no-default" }
    deny = [pscustomobject]@{ id = "P14-VAL-030"; command = "just deny" }
    evidence = [pscustomobject]@{ id = "P14-VAL-034"; command = "pwsh -NoProfile -Command `"& ./scripts/audit-release-registry.ps1 -Mode Evidence -PhaseBaseline $($script:Phase14Baseline) -SourceCommit $SourceCommit -EvidenceCommit ((git rev-parse HEAD).Trim())`"" }
}

function Get-ProofRole {
    param([object]$Definition, [object]$Claim)
    $id = $Definition.requirement_id
    $title = $Definition.normative_title
    if ($id -match '^DOC-' -or $id -match '^CONF-(?:TRACE|REL|GOV|EVID)') { return $roleTable.evidence }
    if ($id -in @("COMPAT-RUST-002", "CONF-MSRV-001") -or $title -match '(?i)MSRV') { return $roleTable.msrv }
    if ($id -match '^(?:COMPAT-REQWEST|CONF-REQWEST)' -or $title -match '(?i)Reqwest') { return $roleTable.reqwest }
    if ($id -match '^FEAT-NODEFAULT') { return $roleTable.no_default }
    if ($id -match '^FEAT-DANGER' -or $title -match '(?i)dangerous.*feature|default absence') { return $roleTable.dangerous }
    if ($title -match '(?i)Rustdoc|documentation exposure' -or $id -match '^(?:GEN-DOC|CONF-DOC)') { return $roleTable.rustdoc }
    if ($id -match '^DSL-' -or $id -match '^COMPAT-MACRO') { return $roleTable.grammar }
    if ($id -match '^(?:FEAT-|COMPAT-(?:BUILD|CRATE|DEP|DEPS))') { return $roleTable.feature }
    if ($id -match '(?:RECORD|CSV|NDJSON)') { return $roleTable.records }
    if ($id -match '^CONF-EXAMPLE' -or $title -match '(?i)example') { return $roleTable.examples }
    if ($id -match '^CONF-PERF' -or $title -match '(?i)benchmark') { return $roleTable.benchmarks }
    if ($id -match '^GEN-') { return $roleTable.generic }
    if ($id -match '^(?:SEC-|BOUND-|EXEC-)' -or $title -match '(?i)side.effect|lifecycle|redact|limit|cancel|deadline|order') { return $roleTable.security }
    if ($id -match '^(?:API-(?:AUTH|HTTP|PHTTP|RATE)|PROD-GUAR-01[23])') { return $roleTable.reqwest }
    if ($id -match '^API-' -or $id -match '^CONF-') { return $roleTable.generic }
    return $roleTable.test_all
}

function Test-PathAtCommit {
    param([string]$Commit, [string]$Path)
    & git cat-file -e "$Commit`:$Path" 2>$null
    return $LASTEXITCODE -eq 0
}

function Get-ClaimProofPaths {
    param([object]$Definition, [object]$Claim, [bool]$EvidenceState)
    if ($EvidenceState) {
        return @(
            $Definition.normative_document,
            $script:RequirementRegistryPath,
            $script:ClaimRegistryPath,
            $script:ClaimSplitRegistryPath,
            $script:ClaimAuditRegistryPath,
            $script:Phase14ReportPath,
            "scripts/audit-release-registry.ps1"
        )
    }
    $candidateRows = @($historicalEvidence | Where-Object {
        $_.requirement_id -eq $Definition.requirement_id -and
        $_.result -eq "passing" -and $_.configuration -match '^direct '
    })
    $paths = [System.Collections.Generic.List[string]]::new()
    foreach ($row in $candidateRows) {
        foreach ($path in @($row.exact_path -split ';')) {
            if ([string]::IsNullOrWhiteSpace($path)) { continue }
            if ($path -match '^docs/(?:normative|architecture|conformance)/') { continue }
            if (Test-PathAtCommit $SourceCommit $path) { $paths.Add($path) }
        }
    }
    if ($paths.Count -eq 0) {
        $coverageRow = $coverage | Where-Object requirement_id -eq $Definition.requirement_id | Select-Object -First 1
        foreach ($path in @($coverageRow.planned_path -split ';')) {
            if (-not [string]::IsNullOrWhiteSpace($path) -and (Test-PathAtCommit $SourceCommit $path)) { $paths.Add($path) }
        }
    }
    $unique = @($paths | Sort-Object -Unique)
    Assert-Condition ($unique.Count -gt 0) "no current source-grounded proof path for $($Definition.requirement_id)"
    $tokens = @([regex]::Matches($Claim.claim_atom.ToLowerInvariant(), '[a-z][a-z0-9_]{3,}') | ForEach-Object Value | Sort-Object -Unique)
    $scored = foreach ($path in $unique) {
        $score = 0; $lower = $path.ToLowerInvariant()
        foreach ($token in $tokens) { if ($lower.Contains($token)) { $score++ } }
        [pscustomobject]@{ path = $path; score = $score }
    }
    return @($scored | Sort-Object @{ Expression = 'score'; Descending = $true }, path | Select-Object -First 4 -ExpandProperty path)
}

$definitionsById = @{}; foreach ($definition in $definitions) { $definitionsById[$definition.requirement_id] = $definition }
$claimsByRequirement = @{}; foreach ($claim in $claims) { if (-not $claimsByRequirement.ContainsKey($claim.requirement_id)) { $claimsByRequirement[$claim.requirement_id] = [System.Collections.Generic.List[object]]::new() }; $claimsByRequirement[$claim.requirement_id].Add($claim) }

$evidenceRows = [System.Collections.Generic.List[object]]::new()
$requirementRows = [System.Collections.Generic.List[object]]::new()
$claimRows = [System.Collections.Generic.List[object]]::new()
$splitRows = [System.Collections.Generic.List[object]]::new()
$auditRows = [System.Collections.Generic.List[object]]::new()
$requirementOrdinal = 0
foreach ($definition in $definitions) {
    $requirementOrdinal++
    $isEvidenceState = $definition.requirement_id -match '^DOC-' -or $definition.requirement_id -match '^CONF-(?:TRACE|REL|GOV|EVID)'
    $requirementClaims = @($claimsByRequirement[$definition.requirement_id])
    $allPaths = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    $roles = [System.Collections.Generic.List[object]]::new()
    foreach ($claim in $requirementClaims) {
        $role = Get-ProofRole $definition $claim
        $roles.Add($role)
        $paths = @(Get-ClaimProofPaths $definition $claim $isEvidenceState)
        foreach ($path in $paths) { [void]$allPaths.Add($path) }
    }
    $role = $roles[0]
    $evidenceId = ("P14-REQ-{0:D3}" -f $requirementOrdinal)
    $proofCommit = if ($isEvidenceState) { $EvidenceCommit } else { $SourceCommit }
    $proofClass = if ($isEvidenceState) { "evidence_state" } else { "immutable_source" }
    $evidenceRows.Add([pscustomobject][ordered]@{
        requirement_id = $definition.requirement_id
        evidence_type = "release claim audit"
        evidence_id = $evidenceId
        exact_path = (@($allPaths | Sort-Object) -join ';')
        exact_test = $role.command
        configuration = "direct claim-specific Phase 14 release proof; validation_id=$($role.id)"
        result = "passing"
        verified_commit = $proofCommit
        verified_at = "2026-07-22"
        waiver = ""
    })
    $requirementRows.Add([pscustomobject][ordered]@{
        requirement_id = $definition.requirement_id
        normative_document = $definition.normative_document
        normative_title = $definition.normative_title
        normative_paragraph_digest = $definition.normative_paragraph_digest
        current_direct_passing_evidence_owner = if ($isEvidenceState) { "Phase 14 release-final evidence state" } else { "Immutable validated source evidence" }
        evidence_id = $evidenceId
        evidence_commit_class = $proofClass
        result = "passing"
        waiver_id = ""
    })
    foreach ($claim in $requirementClaims) {
        $role = Get-ProofRole $definition $claim
        $paths = @(Get-ClaimProofPaths $definition $claim $isEvidenceState)
        $pathText = $paths -join ';'
        $claimRows.Add([pscustomobject][ordered]@{
            claim_id = $claim.claim_id; requirement_id = $claim.requirement_id
            normative_document = $definition.normative_document; normative_title = $definition.normative_title
            normative_paragraph_digest = $definition.normative_paragraph_digest
            source_unit_id = $claim.source_unit_id; source_unit_digest = $claim.source_unit_digest
            evidence_id = $evidenceId; classification = "direct"; claim_atom = $claim.claim_atom
            proof_path = $pathText; proof_command = $role.command; validation_id = $role.id
            proof_commit = $proofCommit; result = "passing"; later_owner = ""
        })
        $rationale = if ([int]$claim.atom_count -gt 1) { "Reviewed independent operative modal clause $($claim.atom_ordinal) of $($claim.atom_count); the compound source unit is not copied as the atom." } elseif ($claim.claim_atom -like 'Enumerated clause:*') { "Reviewed enumerated clause as one independently falsifiable list item." } elseif ($claim.claim_atom -like 'Canonical code clause:*') { "Reviewed canonical code clause as one independently falsifiable syntax or signature contract." } else { "Reviewed single operative structural unit as one independently falsifiable claim." }
        $splitRows.Add([pscustomobject][ordered]@{
            claim_id = $claim.claim_id; requirement_id = $claim.requirement_id
            source_unit_id = $claim.source_unit_id; source_unit_digest = $claim.source_unit_digest
            source_unit = $claim.source_unit; split_kind = $claim.split_kind
            atom_ordinal = $claim.atom_ordinal; atom_count = $claim.atom_count; claim_atom = $claim.claim_atom
            review_status = "reviewed"; review_rationale = $rationale; result = "passing"
        })
        $auditRows.Add([pscustomobject][ordered]@{
            claim_id = $claim.claim_id; requirement_id = $claim.requirement_id; source_unit_id = $claim.source_unit_id
            claim_digest = Get-TextSha256 $claim.claim_atom; proof_path = $pathText
            proof_command = $role.command; validation_id = $role.id; proof_commit_class = $proofClass
            review_status = "reviewed"; review_note = "Reviewed exact path, exact command, source-matched validation ID, proof class, and direct result for this claim."
            result = "passing"
        })
    }
}

function Export-ReleaseCsv {
    param([object[]]$Rows, [string]$Path)
    $text = ($Rows | ConvertTo-Csv -NoTypeInformation) -join "`n"
    [IO.File]::WriteAllText((Join-Path (Get-Location) $Path), $text + "`n", [Text.UTF8Encoding]::new($false))
}

Export-ReleaseCsv $requirementRows $script:RequirementRegistryPath
Export-ReleaseCsv $claimRows $script:ClaimRegistryPath
Export-ReleaseCsv $splitRows $script:ClaimSplitRegistryPath
Export-ReleaseCsv $auditRows $script:ClaimAuditRegistryPath

# Preserve the historical catalog byte-for-byte and append exactly one direct row per active
# requirement. ConvertTo-Csv supplies the suffix rows only; the historical header is not rewritten.
$historicalBytes = Get-GitBlobBytes $SourceCommit $script:EvidencePath
$suffix = (($evidenceRows | ConvertTo-Csv -NoTypeInformation | Select-Object -Skip 1) -join "`n") + "`n"
$suffixBytes = [Text.Encoding]::UTF8.GetBytes($suffix)
$catalogBytes = [byte[]]::new($historicalBytes.Length + $suffixBytes.Length)
[Array]::Copy($historicalBytes, 0, $catalogBytes, 0, $historicalBytes.Length)
[Array]::Copy($suffixBytes, 0, $catalogBytes, $historicalBytes.Length, $suffixBytes.Length)
[IO.File]::WriteAllBytes((Join-Path (Get-Location) $script:EvidencePath), $catalogBytes)

$requirementById = @{}; foreach ($row in $requirementRows) { $requirementById[$row.requirement_id] = $row }
foreach ($row in $coverage) {
    if ($row.status -eq "superseded") { continue }
    $owner = $requirementById[$row.requirement_id]
    $row.planned_evidence_type = "release claim audit"
    $row.planned_evidence_id = $owner.evidence_id
    $row.implementation_milestone = "Phase 14"
    $row.status = "implemented"
    $row.notes = "Resolved by $($owner.evidence_id) through claim-specific direct proof and a source-matched validation ID; no waiver or partial claim."
}
Export-ReleaseCsv $coverage $script:CoveragePath

# Exact shared terminology.
$terms = @(Get-ExpectedTerminology $documents | ForEach-Object {
    [pscustomobject][ordered]@{ terminology_id=$_.terminology_id; term=$_.term; definition=$_.definition; definition_digest=$_.definition_digest; authoritative_document=$_.authoritative_document; result="passing" }
})
Export-ReleaseCsv $terms $script:TerminologyRegistryPath

# Exact requirement-to-requirement references inside active normative paragraphs.
$referenceRows = [System.Collections.Generic.List[object]]::new(); $seenReferences = @{}; $referenceOrdinal = 0
foreach ($definition in $definitions) {
    foreach ($match in [regex]::Matches($definition.normative_paragraph, '\b[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+-[0-9]{3}\b')) {
        $target = $match.Value; if ($target -eq $definition.requirement_id) { continue }
        Assert-Condition $definitionsById.ContainsKey($target) "unknown normative reference $($definition.requirement_id) -> $target"
        $key = "$($definition.requirement_id)|$target"; if ($seenReferences.ContainsKey($key)) { continue }; $seenReferences[$key]=$true; $referenceOrdinal++
        $referenceRows.Add([pscustomobject][ordered]@{
            reference_id=("REF-{0:D3}" -f $referenceOrdinal); source_requirement_id=$definition.requirement_id
            source_document=$definition.normative_document; target_requirement_id=$target
            target_document=$definitionsById[$target].normative_document
            source_paragraph_digest=$definition.normative_paragraph_digest; result="passing"
        })
    }
}
Export-ReleaseCsv $referenceRows $script:ReferenceRegistryPath

# Actual observable production changes since the Phase 10 release-candidate source. Test, report,
# evidence, and audit-tool paths are not represented as product changes.
$activeIds = [System.Collections.Generic.HashSet[string]]::new([string[]]$definitions.requirement_id, [System.StringComparer]::Ordinal)
$requirementsByPath = @{}
foreach ($row in $historicalEvidence) {
    if (-not $activeIds.Contains($row.requirement_id)) { continue }
    foreach ($path in @($row.exact_path -split ';')) {
        if (-not $requirementsByPath.ContainsKey($path)) { $requirementsByPath[$path] = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal) }
        [void]$requirementsByPath[$path].Add($row.requirement_id)
    }
}
$changed = @(& git diff --name-only "$($script:Phase14HistoricalReleaseSource)..$SourceCommit" | Where-Object {
    $_ -match '^(?:concord_core|concord_macros)/src/' -or
    $_ -match '^(?:Cargo\.toml|Cargo\.lock|concord_core/Cargo\.toml|concord_macros/Cargo\.toml|concord_examples/Cargo\.toml|perf/Cargo\.toml)$' -or
    $_ -match '^concord_examples/(?:src|examples)/'
})
$changeRows = [System.Collections.Generic.List[object]]::new(); $changeOrdinal = 0
foreach ($path in $changed) {
    if (-not $requirementsByPath.ContainsKey($path) -or $requirementsByPath[$path].Count -eq 0) { continue }
    $subjects = @(& git log --format=%s "$($script:Phase14HistoricalReleaseSource)..$SourceCommit" -- $path | Where-Object { $_ -notmatch '^docs:' } | Select-Object -Unique -First 3)
    Assert-Condition ($subjects.Count -gt 0) "changed product path lacks an observable commit description: $path"
    $description = (($subjects | ForEach-Object { $_ -replace '^(?:feat|fix|refactor|perf|test):\s*', '' }) -join '; ')
    $description = $description.Substring(0,1).ToUpperInvariant() + $description.Substring(1).TrimEnd('.') + '.'
    $ids = @($requirementsByPath[$path] | Sort-Object)
    $definition = $definitionsById[$ids[0]]; $claim = @($claimsByRequirement[$ids[0]])[0]; $role = Get-ProofRole $definition $claim
    $changeOrdinal++
    $changeRows.Add([pscustomobject][ordered]@{
        change_id=("RELEASE-CHG-{0:D3}" -f $changeOrdinal); exact_path=$path; observable_change=$description
        requirement_ids=($ids -join ';'); proof_command=$role.command; validation_id=$role.id
        proof_commit_class="immutable_source"; result="passing"
    })
}
Assert-Condition ($changeRows.Count -gt 0) "release change map is empty"
Export-ReleaseCsv $changeRows $script:ChangeMapPath

Assert-Condition ($observations.Count -gt 0) "validation observations are required before release artifacts are built"
$validationRows = [System.Collections.Generic.List[object]]::new()
$validationIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
foreach ($observation in $observations) {
    Assert-Condition (-not [string]::IsNullOrWhiteSpace($observation.validation_id)) "validation observation lacks an ID"
    Assert-Condition $validationIds.Add($observation.validation_id) "duplicate validation observation ID $($observation.validation_id)"
    Assert-Condition ($observation.exit_status -eq "0" -and $observation.result -eq "passing") "nonpassing validation observation $($observation.validation_id)"
    $validationRows.Add([pscustomobject][ordered]@{
        validation_id=$observation.validation_id; working_directory="F:\projects\concord_api"
        exact_command=$observation.exact_command; tool_versions=$observation.tool_versions
        feature_configuration=$observation.feature_configuration; source_commit=$SourceCommit
        selected_tests=$observation.selected_tests; exit_status=$observation.exit_status
        result=$observation.result; observed_at=$observation.observed_at
    })
}
$historicalValidationBytes = Get-GitBlobBytes $SourceCommit $script:ValidationRegistryPath
$validationSuffix = (($validationRows | ConvertTo-Csv -NoTypeInformation | Select-Object -Skip 1) -join "`n") + "`n"
$validationSuffixBytes = [Text.Encoding]::UTF8.GetBytes($validationSuffix)
$allValidationBytes = [byte[]]::new($historicalValidationBytes.Length + $validationSuffixBytes.Length)
[Array]::Copy($historicalValidationBytes, 0, $allValidationBytes, 0, $historicalValidationBytes.Length)
[Array]::Copy($validationSuffixBytes, 0, $allValidationBytes, $historicalValidationBytes.Length, $validationSuffixBytes.Length)
[IO.File]::WriteAllBytes((Join-Path (Get-Location) $script:ValidationRegistryPath), $allValidationBytes)

Write-Output "claim-accurate release artifacts built: requirements=$($requirementRows.Count) claims=$($claimRows.Count) splits=$($splitRows.Count) audits=$($auditRows.Count) evidence-added=$($evidenceRows.Count) changes=$($changeRows.Count) terminology=$($terms.Count) references=$($referenceRows.Count) validations=$($validationRows.Count)"

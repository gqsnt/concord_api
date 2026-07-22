param(
    [Parameter(Mandatory = $true)][string]$SourceCommit,
    [string]$OutputPath = "git_diff/phase-14-validation-observations.csv"
)

$ErrorActionPreference = "Stop"
$actualHead = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $actualHead -ne $SourceCommit) { throw "validation must run at exact SourceCommit $SourceCommit" }
if (@(& git status --porcelain).Count -ne 0) { throw "validation requires a clean worktree" }

$toolVersions = @(
    (& rustc --version), (& cargo --version), (& cargo nextest --version), (& just --version),
    (& cargo deny --version), (& pwsh -NoProfile -Command '$PSVersionTable.PSVersion.ToString()'),
    (& powershell.exe -NoLogo -NoProfile -Command '$PSVersionTable.PSVersion.ToString()'), (& git --version)
) -join '; '

$baseline = "ebc25547f339062a8190beb111d354871e276648"
$commands = @(
    [pscustomobject]@{ id="P14-VAL-001"; exact="pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode SelfTest"; config="PowerShell 7 claim/governance self-tests"; run={ & pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode SelfTest } },
    [pscustomobject]@{ id="P14-VAL-002"; exact="powershell.exe -NoLogo -NoProfile -File scripts/audit-release-registry.ps1 -Mode SelfTest"; config="Windows PowerShell 5.1 claim/governance self-tests"; run={ & powershell.exe -NoLogo -NoProfile -File scripts/audit-release-registry.ps1 -Mode SelfTest } },
    [pscustomobject]@{ id="P14-VAL-003"; exact="pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode Source -PhaseBaseline $baseline -SourceCommit $SourceCommit"; config="candidate-state source boundary"; run={ & pwsh -NoProfile -File scripts/audit-release-registry.ps1 -Mode Source -PhaseBaseline $baseline -SourceCommit $SourceCommit } },
    [pscustomobject]@{ id="P14-VAL-004"; exact="cargo nextest run --profile ci -p concord_core --all-features --lib -E 'test(=execute::auth_tests::paginated_tls_recovery_reuses_lexical_plan_across_recovery_and_later_page)'"; config="all features; authenticated pagination recovery"; run={ & cargo nextest run --profile ci -p concord_core --all-features --lib -E 'test(=execute::auth_tests::paginated_tls_recovery_reuses_lexical_plan_across_recovery_and_later_page)' } },
    [pscustomobject]@{ id="P14-VAL-005"; exact="just _test-dangerous-features"; config="dangerous feature isolation"; run={ & just _test-dangerous-features } },
    [pscustomobject]@{ id="P14-VAL-006"; exact="just _feature-matrix"; config="complete feature and fixture topology"; run={ & just _feature-matrix } },
    [pscustomobject]@{ id="P14-VAL-007"; exact="just _generic-spec-record-contracts"; config="generic request/response/record contracts"; run={ & just _generic-spec-record-contracts } },
    [pscustomobject]@{ id="P14-VAL-008"; exact="just _record-feature-topology"; config="records/csv/ndjson isolated features and Rustdoc"; run={ & just _record-feature-topology } },
    [pscustomobject]@{ id="P14-VAL-009"; exact="just _examples"; config="canonical examples"; run={ & just _examples } },
    [pscustomobject]@{ id="P14-VAL-010"; exact="just _benchmarks"; config="benchmark compile and bounded smoke"; run={ & just _benchmarks } },
    [pscustomobject]@{ id="P14-VAL-011"; exact="just _msrv"; config="Rust 1.85 MSRV roles"; run={ & just _msrv } },
    [pscustomobject]@{ id="P14-VAL-012"; exact="just _grammar-and-diagnostics"; config="DSL grammar, semantic diagnostics, and Trybuild"; run={ & just _grammar-and-diagnostics } },
    [pscustomobject]@{ id="P14-VAL-013"; exact="just _reqwest-assumptions"; config="Reqwest dependency and runtime assumptions"; run={ & just _reqwest-assumptions } },
    [pscustomobject]@{ id="P14-VAL-014"; exact="just _security-and-properties"; config="security, limits, lifecycle, and reproducible properties"; run={ & just _security-and-properties } },
    [pscustomobject]@{ id="P14-VAL-015"; exact="just _rustdoc-audits"; config="public and generated Rustdoc"; run={ & just _rustdoc-audits } },
    [pscustomobject]@{ id="P14-VAL-016"; exact="just _release-audits"; config="release naming, artifacts, self-tests, and source boundary"; run={ & just _release-audits } },
    [pscustomobject]@{ id="P14-VAL-017"; exact="just fmt-check"; config="workspace formatting"; run={ & just fmt-check } },
    [pscustomobject]@{ id="P14-VAL-018"; exact="just check"; config="workspace default features"; run={ & just check } },
    [pscustomobject]@{ id="P14-VAL-019"; exact="just check-all"; config="workspace all features"; run={ & just check-all } },
    [pscustomobject]@{ id="P14-VAL-020"; exact="just check-no-default"; config="concord_core no default features"; run={ & just check-no-default } },
    [pscustomobject]@{ id="P14-VAL-021"; exact="just clippy"; config="default features; warnings denied"; run={ & just clippy } },
    [pscustomobject]@{ id="P14-VAL-022"; exact="just clippy-all"; config="all features; warnings denied"; run={ & just clippy-all } },
    [pscustomobject]@{ id="P14-VAL-023"; exact="just test"; config="workspace default Nextest"; run={ & just test } },
    [pscustomobject]@{ id="P14-VAL-024"; exact="just test-all"; config="workspace all-feature Nextest"; run={ & just test-all } },
    [pscustomobject]@{ id="P14-VAL-025"; exact="just test-no-default"; config="no-default HTTP semantic test"; run={ & just test-no-default } },
    [pscustomobject]@{ id="P14-VAL-026"; exact="just doctest"; config="workspace default doctests"; run={ & just doctest } },
    [pscustomobject]@{ id="P14-VAL-027"; exact="just doctest-all"; config="workspace all-feature doctests"; run={ & just doctest-all } },
    [pscustomobject]@{ id="P14-VAL-028"; exact="just doc"; config="four default-feature Rustdoc roles; warnings denied"; run={ & just doc } },
    [pscustomobject]@{ id="P14-VAL-029"; exact="just doc-all"; config="four all-feature Rustdoc roles; warnings denied"; run={ & just doc-all } },
    [pscustomobject]@{ id="P14-VAL-030"; exact="just deny"; config="advisories, bans, licenses, and sources"; run={ & just deny } },
    [pscustomobject]@{ id="P14-VAL-031"; exact="just diff-check"; config="Git whitespace check"; run={ & just diff-check } },
    [pscustomobject]@{ id="P14-VAL-032"; exact="just release"; config="complete maintained release aggregate"; run={ & just release } }
)

$outputFullPath = Join-Path (Get-Location) $OutputPath
$logDirectory = Join-Path (Get-Location) "git_diff/phase-14-validation-logs"
[void][IO.Directory]::CreateDirectory($logDirectory)
$rows = [System.Collections.Generic.List[object]]::new()
foreach ($command in $commands) {
    Write-Output "PHASE14_VALIDATION_START $($command.id) $($command.exact)"
    $logPath = Join-Path $logDirectory "$($command.id).log"
    $started = [DateTime]::UtcNow
    & $command.run 2>&1 | Tee-Object -FilePath $logPath
    $exit = $LASTEXITCODE
    if ($null -eq $exit) { $exit = 0 }
    $completed = [DateTime]::UtcNow
    $log = Get-Content -Raw -LiteralPath $logPath
    $summaries = @([regex]::Matches($log, '(?im)^.*(?:tests? run|tests? passed|passed \[[^\]]+\]).*$') | ForEach-Object { $_.Value.Trim() } | Select-Object -Last 6)
    $selected = if ($summaries.Count -gt 0) { $summaries -join ' | ' } else { "completed exact maintained role in $([Math]::Round(($completed-$started).TotalSeconds,3)) seconds" }
    $row = [pscustomobject][ordered]@{
        validation_id=$command.id; exact_command=$command.exact; tool_versions=$toolVersions
        feature_configuration=$command.config; selected_tests=$selected; exit_status=[string]$exit
        result=if ($exit -eq 0) { "passing" } else { "failing" }
        observed_at=$completed.ToString("yyyy-MM-ddTHH:mm:ssZ")
    }
    $rows.Add($row)
    $text = ($rows | ConvertTo-Csv -NoTypeInformation) -join "`n"
    [IO.File]::WriteAllText($outputFullPath, $text + "`n", [Text.UTF8Encoding]::new($false))
    if ($exit -ne 0) { throw "Phase 14 validation failed: $($command.id) $($command.exact)" }
    Write-Output "PHASE14_VALIDATION_PASS $($command.id)"
}

Write-Output "Phase 14 validation complete: source=$SourceCommit commands=$($rows.Count) observations=$OutputPath"

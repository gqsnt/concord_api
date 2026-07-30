[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$docs = Join-Path $root 'docs'
$fixture = Join-Path $root 'crates/concord-examples/src/docs_spec.rs'
$dslLanguage = 'concord'
$dslStart = '^\s*(?:api!|client\s|scope\s|(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|TRACE|CONNECT|METHOD)\s|vars\s*\{|credentials\s*\{|host\s|path\s|query\s*\{|headers\s*\{|auth\s|timeout\s|paginate\s|rate_limits\s*\{|rate_limit\s|fmt\[)'

function Get-NormalizedSource {
    param([Parameter(Mandatory)][string]$Source)

    return [regex]::Replace($Source, '\s+', '')
}

function Get-Fences {
    param([Parameter(Mandatory)][string]$Path)

    $result = [System.Collections.Generic.List[object]]::new()
    $language = $null
    $start = 0
    $body = [System.Collections.Generic.List[string]]::new()
    $lines = Get-Content -LiteralPath $Path

    for ($index = 0; $index -lt $lines.Count; $index++) {
        $line = $lines[$index]
        if ($null -eq $language) {
            if ($line -match '^```([A-Za-z0-9_-]*)\s*$') {
                $language = $Matches[1]
                $start = $index + 2
                $body = [System.Collections.Generic.List[string]]::new()
            }
            continue
        }

        if ($line -eq '```') {
            $result.Add([pscustomobject]@{
                Language = $language
                Start = $start
                Body = $body -join "`n"
            })
            $language = $null
            continue
        }
        $body.Add($line)
    }

    if ($null -ne $language) {
        $relative = [System.IO.Path]::GetRelativePath($root, $Path)
        throw "${relative}:$($start - 1): unclosed fence"
    }
    return $result
}

$fixtureSource = Get-Content -LiteralPath $fixture -Raw
$normalizedFixture = Get-NormalizedSource -Source $fixtureSource
$errors = [System.Collections.Generic.List[string]]::new()
$checked = 0

foreach ($file in Get-ChildItem -LiteralPath $docs -Recurse -File -Filter '*.md' | Sort-Object FullName) {
    $relative = [System.IO.Path]::GetRelativePath($root, $file.FullName)
    foreach ($block in Get-Fences -Path $file.FullName) {
        $firstCodeLine = ($block.Body -split "`r?`n" | Where-Object { $_.Trim() } | Select-Object -First 1)
        $looksLikeDsl = $firstCodeLine -match $dslStart
        $location = "${relative}:$($block.Start)"

        if ($looksLikeDsl -and $block.Language -ne $dslLanguage) {
            $errors.Add("${location}: DSL fence must use ``````${dslLanguage}")
        }
        if ($block.Language -ne $dslLanguage) {
            continue
        }

        $checked++
        if ([string]::IsNullOrWhiteSpace($block.Body)) {
            $errors.Add("${location}: empty DSL fence")
        } elseif (-not $normalizedFixture.Contains((Get-NormalizedSource -Source $block.Body))) {
            $fixtureRelative = [System.IO.Path]::GetRelativePath($root, $fixture)
            $errors.Add("${location}: normalized DSL block is absent from ${fixtureRelative}")
        }
    }
}

if ($errors.Count -gt 0) {
    foreach ($message in $errors) {
        [Console]::Error.WriteLine($message)
    }
    exit 1
}

$fixtureRelative = [System.IO.Path]::GetRelativePath($root, $fixture)
Write-Output "validated ${checked} fenced DSL examples against ${fixtureRelative}"

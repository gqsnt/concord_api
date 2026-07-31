#requires -Version 7.0
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$docs = Join-Path $root 'docs'
$docsIndex = Join-Path $docs 'README.md'
$fixture = Join-Path $root 'crates/concord-examples/src/docs_spec.rs'
$dslLanguage = 'concord'
$dslStart = '^\s*(?:api!|client\s|scope\s|(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|TRACE|CONNECT|METHOD)\s|vars\s*\{|credentials\s*\{|host\s|path\s|query\s*\{|headers\s*\{|auth\s|timeout\s|paginate\s|rate_limits\s*\{|rate_limit\s|fmt\[)'
$sourceMarker = '^\s*<!--\s*concord-source:\s*([^#\s]+)#([A-Za-z0-9_-]+)\s*-->\s*$'
$removedDocs = @(
    'docs/advanced_endpoints.md',
    'docs/bodies_and_responses.md',
    'docs/retry_and_rate_limit.md',
    'docs/customization.md'
)

# Source anchors may resolve only inside these roots. Both are compiled: `src`
# by the `concord-examples` lib target, `tests` by its integration test targets.
# Adding a root requires a documented reason and a compiled Cargo target.
$anchorRoots = @(
    'crates/concord-examples/src',
    'crates/concord-examples/tests'
)
$anchorExtension = '.rs'
$anchorRegionCache = @{}
$headingSlugCache = @{}
$separator = [System.IO.Path]::DirectorySeparatorChar
$rootFull = [System.IO.Path]::GetFullPath($root).TrimEnd($separator)
$rootPrefix = $rootFull + $separator

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
    $pendingSource = $null
    $lines = Get-Content -LiteralPath $Path

    for ($index = 0; $index -lt $lines.Count; $index++) {
        $line = $lines[$index]
        if ($null -eq $language) {
            if ($line -match $sourceMarker) {
                $pendingSource = [pscustomobject]@{ Path = $Matches[1]; Anchor = $Matches[2]; Line = $index + 1 }
                continue
            }
            if ($line -match '^```([A-Za-z0-9_-]*)\s*$') {
                $language = $Matches[1]
                $start = $index + 2
                $body = [System.Collections.Generic.List[string]]::new()
                continue
            }
            if ($null -ne $pendingSource -and $line.Trim()) {
                $relative = [System.IO.Path]::GetRelativePath($root, $Path)
                throw "${relative}:$($pendingSource.Line): concord-source marker must immediately precede a fence"
            }
            continue
        }

        if ($line -match '^```+\s*$') {
            $result.Add([pscustomobject]@{
                Language = $language
                Start = $start
                Body = $body -join "`n"
                Source = $pendingSource
            })
            $language = $null
            $pendingSource = $null
            continue
        }
        $body.Add($line)
    }

    if ($null -ne $language) {
        $relative = [System.IO.Path]::GetRelativePath($root, $Path)
        throw "${relative}:$($start - 1): unclosed fence"
    }
    if ($null -ne $pendingSource) {
        $relative = [System.IO.Path]::GetRelativePath($root, $Path)
        throw "${relative}:$($pendingSource.Line): concord-source marker has no fence"
    }
    return $result
}

function Resolve-AnchorSourcePath {
    param([Parameter(Mandatory)][string]$RelativePath)

    if ([string]::IsNullOrWhiteSpace($RelativePath)) {
        throw 'source path is empty'
    }
    # Documentation metadata is untrusted input. Reject anything that is not a
    # plain repository-relative path before touching the filesystem.
    if ([System.IO.Path]::IsPathRooted($RelativePath) -or $RelativePath -match '^[A-Za-z]:' -or $RelativePath.StartsWith('\\')) {
        throw 'source path must be repository-relative, not absolute'
    }
    if ($RelativePath -split '[\\/]' -contains '..') {
        throw 'source path must not traverse upward with ".."'
    }

    $resolved = [System.IO.Path]::GetFullPath((Join-Path $root $RelativePath))
    if (-not $resolved.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'source path escapes the repository root'
    }
    if ([System.IO.Path]::GetExtension($resolved) -ne $anchorExtension) {
        throw "source path must be a ${anchorExtension} file"
    }

    $approved = $false
    foreach ($candidate in $anchorRoots) {
        $candidateFull = [System.IO.Path]::GetFullPath((Join-Path $root $candidate)).TrimEnd($separator) + $separator
        if ($resolved.StartsWith($candidateFull, [System.StringComparison]::OrdinalIgnoreCase)) {
            $approved = $true
            break
        }
    }
    if (-not $approved) {
        throw "source path is outside the approved anchor roots ($($anchorRoots -join ', '))"
    }
    if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
        throw 'source file does not exist'
    }
    return $resolved
}

function Get-AnchorRegions {
    param(
        [Parameter(Mandatory)][string]$FullPath,
        [Parameter(Mandatory)][string]$RelativePath
    )

    if ($anchorRegionCache.ContainsKey($FullPath)) {
        return $anchorRegionCache[$FullPath]
    }

    $lines = @(Get-Content -LiteralPath $FullPath)
    $openStack = [System.Collections.Generic.List[object]]::new()
    $regions = @{}

    for ($index = 0; $index -lt $lines.Count; $index++) {
        $text = $lines[$index].Trim()
        if ($text -match '^//\s*ANCHOR:\s*([A-Za-z0-9_-]+)\s*$') {
            $name = $Matches[1]
            $alreadyOpen = @($openStack | Where-Object { $_.Name -eq $name }).Count -gt 0
            if ($regions.ContainsKey($name) -or $alreadyOpen) {
                throw "duplicate anchor '${name}' in ${RelativePath}"
            }
            $openStack.Add([pscustomobject]@{ Name = $name; Start = $index })
            continue
        }
        if ($text -match '^//\s*ANCHOR_END:\s*([A-Za-z0-9_-]+)\s*$') {
            $name = $Matches[1]
            if ($openStack.Count -eq 0) {
                throw "anchor '${name}' in ${RelativePath} is closed but was never opened"
            }
            $innermost = $openStack[$openStack.Count - 1]
            if ($innermost.Name -ne $name) {
                throw "anchor regions overlap in ${RelativePath}: '${name}' closes while '$($innermost.Name)' is still open"
            }
            $openStack.RemoveAt($openStack.Count - 1)
            # Proper nesting is deliberately supported so one fixture can serve
            # both a whole-contract quotation and a single-endpoint quotation.
            $regions[$name] = [pscustomobject]@{ Start = $innermost.Start; End = $index }
            continue
        }
    }

    if ($openStack.Count -gt 0) {
        $unclosed = $openStack[$openStack.Count - 1].Name
        throw "unclosed anchor '${unclosed}' in ${RelativePath}"
    }

    $anchorRegionCache[$FullPath] = $regions
    return $regions
}

function Get-AnchoredSource {
    param(
        [Parameter(Mandatory)][string]$RelativePath,
        [Parameter(Mandatory)][string]$Anchor
    )

    $resolved = Resolve-AnchorSourcePath -RelativePath $RelativePath
    $regions = Get-AnchorRegions -FullPath $resolved -RelativePath $RelativePath
    if (-not $regions.ContainsKey($Anchor)) {
        throw "anchor '${Anchor}' is not defined in ${RelativePath}"
    }

    $region = $regions[$Anchor]
    $lines = @(Get-Content -LiteralPath $resolved)
    $body = [System.Collections.Generic.List[string]]::new()
    for ($index = $region.Start + 1; $index -lt $region.End; $index++) {
        # Nested markers are tooling state, never documented source.
        if ($lines[$index].Trim() -match '^//\s*ANCHOR(_END)?:\s*[A-Za-z0-9_-]+\s*$') {
            continue
        }
        $body.Add($lines[$index])
    }
    if ([string]::IsNullOrWhiteSpace(($body -join "`n"))) {
        throw "anchor '${Anchor}' in ${RelativePath} selects no source lines"
    }
    return $body -join "`n"
}

function ConvertTo-HeadingSlug {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Text)

    $slug = $Text
    $slug = [regex]::Replace($slug, '!?\[([^\]]*)\]\([^)]*\)', '$1')
    $slug = $slug -replace '`', ''
    $slug = [regex]::Replace($slug, '(\*\*|__|~~|\*)', '')
    $slug = $slug.Trim().ToLowerInvariant()
    $slug = [regex]::Replace($slug, '[^\p{L}\p{Nd} _-]', '')
    return ($slug -replace ' ', '-')
}

function Get-HeadingSlugs {
    param([Parameter(Mandatory)][string]$FullPath)

    if ($headingSlugCache.ContainsKey($FullPath)) {
        return $headingSlugCache[$FullPath]
    }

    $slugs = [System.Collections.Generic.List[string]]::new()
    $duplicates = [System.Collections.Generic.List[string]]::new()
    $inFence = $false
    foreach ($line in @(Get-Content -LiteralPath $FullPath)) {
        if ($line -match '^\s*```') {
            $inFence = -not $inFence
            continue
        }
        if ($inFence) {
            continue
        }
        if ($line -match '^(#{1,6})\s+(.*?)\s*#*\s*$') {
            $slug = ConvertTo-HeadingSlug -Text $Matches[2]
            if (-not $slug) {
                continue
            }
            if ($slugs.Contains($slug)) {
                if (-not $duplicates.Contains($slug)) {
                    $duplicates.Add($slug)
                }
            }
            $slugs.Add($slug)
        }
    }

    $result = [pscustomobject]@{ Slugs = $slugs; Duplicates = $duplicates }
    $headingSlugCache[$FullPath] = $result
    return $result
}

function Get-MarkdownLinks {
    param([Parameter(Mandatory)][string]$Path)
    $raw = Get-Content -LiteralPath $Path -Raw
    # A link shape inside a fenced block or an inline code span is illustrative
    # text, not a reference. Fences are removed before inline spans so that
    # fence delimiters are not mistaken for span delimiters.
    $raw = [regex]::Replace($raw, '(?ms)^```.*?^```[^\r\n]*$', '')
    # Inline spans never cross a line, so strip them per line. This keeps an
    # unbalanced backtick on one line from shifting pairing everywhere else.
    $raw = (($raw -split "`r?`n") | ForEach-Object {
        [regex]::Replace($_, '(`+)[^`]*?\1', '')
    }) -join "`n"
    foreach ($match in [regex]::Matches($raw, '!?(?:\[[^\]]*\])\(([^)]+)\)')) {
        $target = $match.Groups[1].Value.Trim()
        if ($target.StartsWith('<') -and $target.EndsWith('>')) {
            $target = $target.Substring(1, $target.Length - 2)
        }
        [pscustomobject]@{ Target = $target; Index = $match.Index }
    }
}

$fixtureSource = Get-Content -LiteralPath $fixture -Raw
$normalizedFixture = Get-NormalizedSource -Source $fixtureSource
$errors = [System.Collections.Generic.List[string]]::new()
$checkedDsl = 0
$checkedAnchors = 0
$checkedLinks = 0
$checkedFragments = 0

$markdownFiles = @(
    Get-Item -LiteralPath (Join-Path $root 'README.md')
) + @(
    Get-Item -LiteralPath (Join-Path $root 'FINAL.md')
) + @(
    Get-ChildItem -LiteralPath $docs -Recurse -File -Filter '*.md'
) + @(
    Get-ChildItem -LiteralPath (Join-Path $root 'dev_doc') -Recurse -File -Filter '*.md'
)

foreach ($file in $markdownFiles | Sort-Object FullName -Unique) {
    $relative = [System.IO.Path]::GetRelativePath($root, $file.FullName)
    foreach ($block in Get-Fences -Path $file.FullName) {
        $firstCodeLine = ($block.Body -split "`r?`n" | Where-Object { $_.Trim() } | Select-Object -First 1)
        $looksLikeDsl = $firstCodeLine -match $dslStart
        $location = "${relative}:$($block.Start)"

        if ($looksLikeDsl -and $block.Language -ne $dslLanguage) {
            $errors.Add("${location}: DSL fence must use ``````${dslLanguage}")
        }

        if ($null -ne $block.Source) {
            $anchorRef = "$($block.Source.Path)#$($block.Source.Anchor)"
            try {
                $expected = Get-AnchoredSource -RelativePath $block.Source.Path -Anchor $block.Source.Anchor
                if ((Get-NormalizedSource -Source $block.Body) -ne (Get-NormalizedSource -Source $expected)) {
                    $errors.Add("${location}: fenced source differs from ${anchorRef}")
                }
                $checkedAnchors++
            } catch {
                $errors.Add("${location}: anchor ${anchorRef}: $($_.Exception.Message)")
            }
            continue
        }

        if ($block.Language -eq $dslLanguage) {
            $checkedDsl++
            if ([string]::IsNullOrWhiteSpace($block.Body)) {
                $errors.Add("${location}: empty DSL fence")
            } elseif (-not $normalizedFixture.Contains((Get-NormalizedSource -Source $block.Body))) {
                $fixtureRelative = [System.IO.Path]::GetRelativePath($root, $fixture)
                $errors.Add("${location}: normalized DSL block is absent from ${fixtureRelative}")
            }
        }
    }

    foreach ($link in Get-MarkdownLinks -Path $file.FullName) {
        $target = $link.Target
        if (-not $target -or $target.StartsWith('#') -or $target -match '^[A-Za-z][A-Za-z0-9+.-]*:') {
            continue
        }
        $filePart = ($target -split '#', 2)[0]
        if (-not $filePart) {
            continue
        }
        $resolved = [System.IO.Path]::GetFullPath((Join-Path $file.DirectoryName $filePart))
        $rootPrefix = [System.IO.Path]::GetFullPath($root).TrimEnd([System.IO.Path]::DirectorySeparatorChar) +
            [System.IO.Path]::DirectorySeparatorChar
        if (-not $resolved.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            $errors.Add("${relative}: link escapes repository root: ${target}")
            continue
        }
        if (-not (Test-Path -LiteralPath $resolved)) {
            $errors.Add("${relative}: missing local link target: ${target}")
            continue
        }
        $checkedLinks++

        $fragment = ($target -split '#', 2)
        if ($fragment.Count -eq 2 -and $fragment[1] -and
            [System.IO.Path]::GetExtension($resolved).Equals('.md', [System.StringComparison]::OrdinalIgnoreCase)) {
            $headings = Get-HeadingSlugs -FullPath $resolved
            $targetRelative = [System.IO.Path]::GetRelativePath($root, $resolved)
            if ($headings.Duplicates.Contains($fragment[1])) {
                $errors.Add("${relative}: ambiguous fragment '#$($fragment[1])': ${targetRelative} has duplicate headings with that slug")
            } elseif (-not $headings.Slugs.Contains($fragment[1])) {
                $errors.Add("${relative}: ${targetRelative} has no heading for fragment '#$($fragment[1])'")
            } else {
                $checkedFragments++
            }
        }
    }

    if ($file.FullName -eq (Join-Path $root 'README.md') -or $file.FullName.StartsWith([System.IO.Path]::GetFullPath($docs), [System.StringComparison]::OrdinalIgnoreCase)) {
        $raw = Get-Content -LiteralPath $file.FullName -Raw
        foreach ($removed in $removedDocs) {
            if ($raw.Contains($removed) -or $raw.Contains([System.IO.Path]::GetFileName($removed))) {
                $errors.Add("${relative}: references removed documentation file ${removed}")
            }
        }
    }
}

if (-not (Test-Path -LiteralPath $docsIndex -PathType Leaf)) {
    $errors.Add('docs/README.md: canonical documentation index is missing')
} else {
    $indexTargets = [System.Collections.Generic.List[string]]::new()
    foreach ($link in Get-MarkdownLinks -Path $docsIndex) {
        $target = $link.Target
        if (-not $target -or $target.StartsWith('#') -or $target -match '^[A-Za-z][A-Za-z0-9+.-]*:') {
            continue
        }
        $filePart = ($target -split '#', 2)[0]
        if (-not $filePart) {
            continue
        }
        $resolved = [System.IO.Path]::GetFullPath((Join-Path $docs $filePart))
        $indexTargets.Add($resolved)
    }
    foreach ($doc in Get-ChildItem -LiteralPath $docs -File -Filter '*.md' | Where-Object Name -ne 'README.md') {
        $count = @($indexTargets | Where-Object { $_ -eq $doc.FullName }).Count
        if ($count -ne 1) {
            $errors.Add("docs/README.md: $($doc.Name) must be linked exactly once; found ${count}")
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
Write-Output "validated ${checkedDsl} DSL fences against ${fixtureRelative}"
Write-Output "validated ${checkedAnchors} source-anchored examples in $($anchorRoots -join ', ')"
Write-Output "validated ${checkedLinks} local Markdown links and ${checkedFragments} heading fragments"
Write-Output 'validated canonical docs/README.md inventory coverage'

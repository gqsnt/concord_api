param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[a-zA-Z0-9_.-]+$')]
    [string]$Label,

    [Parameter(Mandatory = $true)]
    [string]$FilePath,

    [Parameter(Mandatory = $true)]
    [string[]]$CommandArguments,

    [Parameter(Mandatory = $true)]
    [string]$CommandWorkingDirectory,

    [string]$OutputDirectory = 'target/evidence/raw'
)

$ErrorActionPreference = 'Stop'
$resolvedWorkingDirectory = (Resolve-Path -LiteralPath $CommandWorkingDirectory).Path
$resolvedOutputDirectory = [System.IO.Path]::GetFullPath(
    (Join-Path -Path (Get-Location).Path -ChildPath $OutputDirectory)
)
[System.IO.Directory]::CreateDirectory($resolvedOutputDirectory) | Out-Null

$stdoutPath = Join-Path $resolvedOutputDirectory "$Label.stdout.txt"
$stderrPath = Join-Path $resolvedOutputDirectory "$Label.stderr.txt"
$jsonPath = Join-Path $resolvedOutputDirectory "$Label.json"

$process = Start-Process `
    -FilePath $FilePath `
    -ArgumentList $CommandArguments `
    -WorkingDirectory $resolvedWorkingDirectory `
    -WindowStyle Hidden `
    -RedirectStandardOutput $stdoutPath `
    -RedirectStandardError $stderrPath `
    -PassThru

$started = [System.Diagnostics.Stopwatch]::StartNew()
$peakWorkingSetBytes = [int64]0
$peakPrivateBytes = [int64]0

while (-not $process.HasExited) {
    $rows = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $parents = @{}
    foreach ($row in $rows) {
        $parents[[int]$row.ProcessId] = [int]$row.ParentProcessId
    }

    $treeIds = [System.Collections.Generic.HashSet[int]]::new()
    [void]$treeIds.Add([int]$process.Id)
    foreach ($row in $rows) {
        $candidate = [int]$row.ProcessId
        $cursor = $candidate
        while ($parents.ContainsKey($cursor) -and $parents[$cursor] -ne 0) {
            $cursor = $parents[$cursor]
            if ($cursor -eq $process.Id) {
                [void]$treeIds.Add($candidate)
                break
            }
        }
    }

    $workingSetBytes = [int64]0
    $privateBytes = [int64]0
    foreach ($processId in $treeIds) {
        $sample = Get-Process -Id $processId -ErrorAction SilentlyContinue
        if ($null -ne $sample) {
            $workingSetBytes += [int64]$sample.WorkingSet64
            $privateBytes += [int64]$sample.PrivateMemorySize64
        }
    }
    $peakWorkingSetBytes = [Math]::Max($peakWorkingSetBytes, $workingSetBytes)
    $peakPrivateBytes = [Math]::Max($peakPrivateBytes, $privateBytes)
    Start-Sleep -Milliseconds 250
    $process.Refresh()
}

$process.WaitForExit()
$started.Stop()
$record = [ordered]@{
    label                  = $Label
    command                = $FilePath
    arguments              = $CommandArguments
    working_directory      = $resolvedWorkingDirectory
    exit_code              = $process.ExitCode
    elapsed_milliseconds   = $started.ElapsedMilliseconds
    peak_working_set_bytes = $peakWorkingSetBytes
    peak_private_bytes     = $peakPrivateBytes
    stdout                 = $stdoutPath
    stderr                 = $stderrPath
}
$record | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath $jsonPath -Encoding utf8
$record | Format-List

exit $process.ExitCode

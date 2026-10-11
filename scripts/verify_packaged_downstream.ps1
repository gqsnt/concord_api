param(
    [string]$WorkspaceRoot = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $WorkspaceRoot).Path
$metadata = & cargo metadata --manifest-path (Join-Path $root 'Cargo.toml') --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {
    throw 'cargo metadata failed'
}
$cargoTarget = [System.IO.Path]::GetFullPath([string]$metadata.target_directory)
function PackageAt([string]$RelativeManifest) {
    $manifest = (Resolve-Path -LiteralPath (Join-Path $root $RelativeManifest)).Path
    $package = $metadata.packages | Where-Object {
        [System.IO.Path]::GetFullPath($_.manifest_path) -eq $manifest
    }
    if ($null -eq $package -or @($package).Count -ne 1) {
        throw "could not identify the package at $RelativeManifest"
    }
    return $package
}

$corePackage = PackageAt 'crates/concord-core/Cargo.toml'
$macrosPackage = PackageAt 'crates/concord-macros/Cargo.toml'
$extrasPackage = PackageAt 'crates/concord-extras/Cargo.toml'
$facadePackage = PackageAt 'crates/concord/Cargo.toml'
$version = $facadePackage.version
$publicPackages = @($corePackage, $macrosPackage, $extrasPackage, $facadePackage)
if ($publicPackages.Where({ $_.version -ne $version }).Count -ne 0) {
    throw 'public package versions are not exact and uniform'
}
$facadeLibraries = @($facadePackage.targets | Where-Object { $_.kind -contains 'lib' })
if ($facadeLibraries.Count -ne 1 -or $facadeLibraries[0].name -ne 'concord') {
    throw 'the public facade must retain the Rust library target name concord'
}

$coreName = $corePackage.name
$macrosName = $macrosPackage.name
$extrasName = $extrasPackage.name
$facadeName = $facadePackage.name
$packages = @($coreName, $macrosName, $extrasName, $facadeName)
$coreSource = (Join-Path $root 'crates/concord-core').Replace('\', '/')
$macrosSource = (Join-Path $root 'crates/concord-macros').Replace('\', '/')
$extrasSource = (Join-Path $root 'crates/concord-extras').Replace('\', '/')
$packagePatches = @(
    "patch.crates-io.$coreName.path='$coreSource'",
    "patch.crates-io.$macrosName.path='$macrosSource'",
    "patch.crates-io.$extrasName.path='$extrasSource'"
)
foreach ($packageName in $packages) {
    $listArguments = @(
        'package', '--manifest-path', (Join-Path $root 'Cargo.toml'),
        '-p', $packageName, '--list', '--locked'
    )
    foreach ($patch in $packagePatches) {
        $listArguments += @('--config', $patch)
    }
    $packageList = @(& cargo @listArguments)
    if ($LASTEXITCODE -ne 0) {
        throw "cargo package --list failed for $packageName"
    }
    if ($packageList -notcontains 'Cargo.toml' -or $packageList -notcontains 'src/lib.rs') {
        throw "package $packageName is missing its manifest or library root"
    }
    $forbiddenEntries = @($packageList | Where-Object {
        $_ -match '(^|/)(dev_doc|concord_v2_1|raw_benchmarks)(/|$)' -or
        $_ -match '(^|/)PROMPT[^/]*$'
    })
    if ($forbiddenEntries.Count -ne 0) {
        throw "package $packageName contains process artifacts: $($forbiddenEntries -join ', ')"
    }

    $packageArguments = @(
        'package', '--manifest-path', (Join-Path $root 'Cargo.toml'),
        '-p', $packageName, '--no-verify', '--locked'
    )
    foreach ($patch in $packagePatches) {
        $packageArguments += @('--config', $patch)
    }
    & cargo @packageArguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo package failed for $packageName"
    }
}

$runRoot = Join-Path ([System.IO.Path]::GetTempPath()) "concord-package-verification/$([Guid]::NewGuid().ToString('N'))"
$unpackedRoot = Join-Path $runRoot 'unpacked'
$downstreamRoot = Join-Path $runRoot 'downstream'
[System.IO.Directory]::CreateDirectory($unpackedRoot) | Out-Null
[System.IO.Directory]::CreateDirectory((Join-Path $downstreamRoot 'src')) | Out-Null

foreach ($packageName in $packages) {
    $archive = Join-Path $cargoTarget "package/$packageName-$version.crate"
    if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
        throw "missing package archive $archive"
    }
    & tar -xzf $archive -C $unpackedRoot
    if ($LASTEXITCODE -ne 0) {
        throw "could not extract $archive"
    }
}

function CargoPath([string]$Path) {
    return $Path.Replace('\', '/')
}

$corePath = CargoPath (Join-Path $unpackedRoot "$coreName-$version")
$macrosPath = CargoPath (Join-Path $unpackedRoot "$macrosName-$version")
$extrasPath = CargoPath (Join-Path $unpackedRoot "$extrasName-$version")
$concordPath = CargoPath (Join-Path $unpackedRoot "$facadeName-$version")
$extractedPatches = @(
    "patch.crates-io.$coreName.path='$corePath'",
    "patch.crates-io.$macrosName.path='$macrosPath'",
    "patch.crates-io.$extrasName.path='$extrasPath'",
    "patch.crates-io.$facadeName.path='$concordPath'"
)

foreach ($packageName in $packages) {
    $packagePath = CargoPath (Join-Path $unpackedRoot "$packageName-$version")
    $packageManifest = Join-Path $packagePath 'Cargo.toml'
    $packageMetadataArguments = @(
        'metadata', '--manifest-path', $packageManifest,
        '--no-deps', '--format-version', '1'
    )
    foreach ($patch in $extractedPatches) {
        $packageMetadataArguments += @('--config', $patch)
    }
    $packageMetadata = & cargo @packageMetadataArguments | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) {
        throw "extracted cargo metadata failed for $packageName"
    }
    $ownPackage = $packageMetadata.packages | Where-Object { $_.name -eq $packageName }
    $workspacePathDependencies = @($ownPackage.dependencies | Where-Object {
        $null -ne $_.path
    })
    if ($workspacePathDependencies.Count -ne 0) {
        throw "normalized archive for $packageName retains path dependencies"
    }
}

foreach ($documentedPath in @($extrasPath, $concordPath)) {
    $documentedManifest = Join-Path $documentedPath 'Cargo.toml'
    $generateArguments = @('generate-lockfile', '--manifest-path', $documentedManifest)
    foreach ($patch in $extractedPatches) {
        $generateArguments += @('--config', $patch)
    }
    & cargo @generateArguments
    if ($LASTEXITCODE -ne 0) {
        throw "packaged lockfile generation failed for $documentedPath"
    }
    $rustdocArguments = @(
        'rustdoc', '--manifest-path', $documentedManifest,
        '--all-features', '--locked'
    )
    foreach ($patch in $extractedPatches) {
        $rustdocArguments += @('--config', $patch)
    }
    $rustdocArguments += @('--', '-D', 'warnings')
    & cargo @rustdocArguments
    if ($LASTEXITCODE -ne 0) {
        throw "warning-denied packaged Rustdoc failed for $documentedPath"
    }
}

$manifest = @"
[package]
name = "concord-packaged-downstream"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
concord = { package = "$facadeName", version = "=$version", default-features = false, features = ["json", "records"] }
serde_json = "1"

[patch.crates-io]
"$facadeName" = { path = "$concordPath" }
"$coreName" = { path = "$corePath" }
"$extrasName" = { path = "$extrasPath" }
"$macrosName" = { path = "$macrosPath" }
"@
[System.IO.File]::WriteAllText((Join-Path $downstreamRoot 'Cargo.toml'), $manifest)

$source = @'
use concord::inspection::RepresentationSource;
use concord::{Json, api};

api! {
    client PackagedApi {
        base "https://package.example.test/";
        timeout 10s;
        response Json;

        GET item(id: u64) -> serde_json::Value {
            path "items/{id}";
        }
    }
}

fn main() {
    let description = PackagedApi::unbound().item(7).description();
    assert_eq!(description.method(), "GET");
    let contract = description.contract().expect("packaged endpoint contract");
    assert_eq!(
        contract.result().representation().source(),
        RepresentationSource::Inherited("PackagedApi")
    );
}
'@
[System.IO.File]::WriteAllText((Join-Path $downstreamRoot 'src/main.rs'), $source)

& cargo generate-lockfile --manifest-path (Join-Path $downstreamRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) {
    throw 'downstream lockfile generation failed'
}
$downstreamMetadata = & cargo metadata --manifest-path (Join-Path $downstreamRoot 'Cargo.toml') --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {
    throw 'downstream cargo metadata failed'
}
foreach ($packageName in $packages) {
    $resolved = @($downstreamMetadata.packages | Where-Object { $_.name -eq $packageName })
    if ($resolved.Count -ne 1 -or $null -ne $resolved[0].source) {
        throw "downstream did not resolve exactly one extracted $packageName archive"
    }
    $resolvedManifest = [System.IO.Path]::GetFullPath($resolved[0].manifest_path)
    if (-not $resolvedManifest.StartsWith([System.IO.Path]::GetFullPath($unpackedRoot))) {
        throw "downstream $packageName resolved outside the extracted archive root"
    }
}
& cargo check --manifest-path (Join-Path $downstreamRoot 'Cargo.toml') --locked
if ($LASTEXITCODE -ne 0) {
    throw 'downstream packaged-artifact check failed'
}
& cargo run --manifest-path (Join-Path $downstreamRoot 'Cargo.toml') --locked
if ($LASTEXITCODE -ne 0) {
    throw 'downstream packaged-artifact run failed'
}

Write-Output "packaged downstream verification passed: version=$version path=$runRoot"

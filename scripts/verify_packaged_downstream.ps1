param(
    [string]$WorkspaceRoot = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath $WorkspaceRoot).Path
$metadata = & cargo metadata --manifest-path (Join-Path $root 'Cargo.toml') --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {
    throw 'cargo metadata failed'
}
$version = ($metadata.packages | Where-Object { $_.name -eq 'concord' }).version
if ([string]::IsNullOrWhiteSpace($version)) {
    throw 'could not determine the Concord package version'
}

$packages = @('concord-core', 'concord-macros', 'concord-extras', 'concord')
$coreSource = (Join-Path $root 'crates/concord-core').Replace('\', '/')
$macrosSource = (Join-Path $root 'crates/concord-macros').Replace('\', '/')
$extrasSource = (Join-Path $root 'crates/concord-extras').Replace('\', '/')
$packagePatches = @(
    "patch.crates-io.concord-core.path='$coreSource'",
    "patch.crates-io.concord-macros.path='$macrosSource'",
    "patch.crates-io.concord-extras.path='$extrasSource'"
)
foreach ($packageName in $packages) {
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

$runRoot = Join-Path $root "target/package-verification/$([Guid]::NewGuid().ToString('N'))"
$unpackedRoot = Join-Path $runRoot 'unpacked'
$downstreamRoot = Join-Path $runRoot 'downstream'
[System.IO.Directory]::CreateDirectory($unpackedRoot) | Out-Null
[System.IO.Directory]::CreateDirectory((Join-Path $downstreamRoot 'src')) | Out-Null

foreach ($packageName in $packages) {
    $archive = Join-Path $root "target/package/$packageName-$version.crate"
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

$corePath = CargoPath (Join-Path $unpackedRoot "concord-core-$version")
$macrosPath = CargoPath (Join-Path $unpackedRoot "concord-macros-$version")
$extrasPath = CargoPath (Join-Path $unpackedRoot "concord-extras-$version")
$concordPath = CargoPath (Join-Path $unpackedRoot "concord-$version")
$manifest = @"
[package]
name = "concord-packaged-downstream"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
concord = { version = "=$version", default-features = false, features = ["json", "records"] }
serde_json = "1"

[patch.crates-io]
concord = { path = "$concordPath" }
concord-core = { path = "$corePath" }
concord-extras = { path = "$extrasPath" }
concord-macros = { path = "$macrosPath" }
"@
[System.IO.File]::WriteAllText((Join-Path $downstreamRoot 'Cargo.toml'), $manifest)

$source = @'
use concord::{api, prelude::*};

api! {
    client PackagedApi {
        base "https://package.example.test/";
        timeout 10s;
        response Json;

        GET item(id: u64) -> serde_json::Value {
            path ["items", endpoint.id];
        }
    }
}

fn main() {
    let endpoint = PackagedApi::unbound().item(7).description();
    assert_eq!(endpoint.method().as_str(), "GET");
    assert_eq!(endpoint.contract().unwrap().response().representation_origin(), "client");
}
'@
[System.IO.File]::WriteAllText((Join-Path $downstreamRoot 'src/main.rs'), $source)

& cargo generate-lockfile --manifest-path (Join-Path $downstreamRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) {
    throw 'downstream lockfile generation failed'
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

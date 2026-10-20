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
if ($publicPackages.Where({ $_.license -ne 'MIT OR Apache-2.0' }).Count -ne 0) {
    throw 'public packages must declare the owner-selected MIT OR Apache-2.0 license'
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
    foreach ($licenseFile in @('LICENSE-MIT', 'LICENSE-APACHE')) {
        if ($packageList -notcontains $licenseFile) {
            throw "package $packageName is missing $licenseFile"
        }
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
    if ($ownPackage.license -ne 'MIT OR Apache-2.0') {
        throw "normalized archive for $packageName lost its license expression"
    }
    foreach ($licenseFile in @('LICENSE-MIT', 'LICENSE-APACHE')) {
        $packagedLicense = Join-Path $packagePath $licenseFile
        if ((Get-FileHash -LiteralPath $packagedLicense -Algorithm SHA256).Hash -ne
            (Get-FileHash -LiteralPath (Join-Path $root $licenseFile) -Algorithm SHA256).Hash) {
            throw "normalized archive for $packageName changed $licenseFile"
        }
    }
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

# Preserve the existing standalone unbound consumer above. In addition, run
# the genuine two-crate acceptance sources against the extracted public crates.
# Only the non-public loopback test harness comes from the workspace. It has
# no dependency on any public Concord crate; verify that boundary below.
$apiFixture = PackageAt 'tests/downstream/concord-downstream-api/Cargo.toml'
$appFixture = PackageAt 'tests/downstream/concord-downstream-app/Cargo.toml'
$supportFixture = PackageAt 'crates/concord-test-support/Cargo.toml'
if (@($supportFixture.dependencies | Where-Object { $_.name -in $packages }).Count -ne 0) {
    throw 'test support must not resolve public Concord packages from the source workspace'
}
$serdeFixture = @($apiFixture.dependencies | Where-Object { $_.name -eq 'serde' })
$tokioFixture = @($appFixture.dependencies | Where-Object { $_.name -eq 'tokio' })
if ($serdeFixture.Count -ne 1 -or $tokioFixture.Count -ne 1) {
    throw 'could not resolve explicit downstream fixture dependency requirements'
}
$serdeRequirement = $serdeFixture[0].req
$tokioRequirement = $tokioFixture[0].req
$supportPath = CargoPath (Join-Path $root 'crates/concord-test-support')
$twoRoot = Join-Path $runRoot 'two-consumers'
$apiRoot = Join-Path $twoRoot 'tests/downstream/concord-downstream-api'
$appRoot = Join-Path $twoRoot 'tests/downstream/concord-downstream-app'
[System.IO.Directory]::CreateDirectory($apiRoot) | Out-Null
[System.IO.Directory]::CreateDirectory($appRoot) | Out-Null
Copy-Item -LiteralPath (Join-Path $root 'tests/downstream/concord-downstream-api/src') -Destination $apiRoot -Recurse
Copy-Item -LiteralPath (Join-Path $root 'tests/downstream/concord-downstream-app/src') -Destination $appRoot -Recurse
Copy-Item -LiteralPath (Join-Path $root 'tests/downstream/concord-downstream-app/tests') -Destination $appRoot -Recurse
# Preserve the original relative include_str paths and exact recording bytes.
$fixtureRoot = Join-Path $twoRoot 'fixtures/contracts'
[System.IO.Directory]::CreateDirectory($fixtureRoot) | Out-Null
Copy-Item -LiteralPath (Join-Path $root 'fixtures/contracts/v3_catalog_recording.json') -Destination $fixtureRoot
if ((Get-FileHash -LiteralPath (Join-Path $fixtureRoot 'v3_catalog_recording.json') -Algorithm SHA256).Hash -ne
    (Get-FileHash -LiteralPath (Join-Path $root 'fixtures/contracts/v3_catalog_recording.json') -Algorithm SHA256).Hash) {
    throw 'downstream archive recording bytes changed'
}
$twoManifest = @"
[workspace]
members = ["tests/downstream/concord-downstream-api", "tests/downstream/concord-downstream-app"]
resolver = "3"

[patch.crates-io]
"$facadeName" = { path = "$concordPath" }
"$coreName" = { path = "$corePath" }
"$extrasName" = { path = "$extrasPath" }
"$macrosName" = { path = "$macrosPath" }
"@
$apiManifest = @"
[package]
name = "concord-downstream-api"
version = "$version"
edition = "2024"
publish = false

[dependencies]
concord = { package = "$facadeName", version = "=$version", default-features = false, features = ["default-tls", "json"] }
serde = { version = "$serdeRequirement", features = ["derive"] }
"@
$appManifest = @"
[package]
name = "concord-downstream-app"
version = "$version"
edition = "2024"
publish = false

[dependencies]
concord = { package = "$facadeName", version = "=$version", default-features = false, features = ["default-tls", "json"] }
concord-downstream-api = { version = "=$version", path = "../concord-downstream-api" }

[dev-dependencies]
concord-test-support = { path = "$supportPath", default-features = false, features = ["loopback"] }
tokio = { version = "$tokioRequirement", features = ["macros", "rt-multi-thread", "time"] }
"@
$twoManifestPath = Join-Path $twoRoot 'Cargo.toml'
[System.IO.File]::WriteAllText($twoManifestPath, $twoManifest)
[System.IO.File]::WriteAllText((Join-Path $apiRoot 'Cargo.toml'), $apiManifest)
[System.IO.File]::WriteAllText((Join-Path $appRoot 'Cargo.toml'), $appManifest)
& cargo generate-lockfile --manifest-path $twoManifestPath
if ($LASTEXITCODE -ne 0) { throw 'two-consumer archive lockfile generation failed' }
$twoMetadata = & cargo metadata --manifest-path $twoManifestPath --locked --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'two-consumer archive metadata failed' }
foreach ($packageName in $packages) {
    $resolved = @($twoMetadata.packages | Where-Object { $_.name -eq $packageName })
    $expectedManifest = [System.IO.Path]::GetFullPath((Join-Path $unpackedRoot "$packageName-$version/Cargo.toml"))
    if ($resolved.Count -ne 1 -or $null -ne $resolved[0].source -or $resolved[0].version -ne $version -or
        [System.IO.Path]::GetFullPath($resolved[0].manifest_path) -ne $expectedManifest) {
        throw "two consumers did not resolve exactly the extracted $packageName-$version archive"
    }
}
foreach ($fixtureName in @('concord-downstream-api', 'concord-downstream-app')) {
    if (@($twoMetadata.packages | Where-Object { $_.name -eq $fixtureName }).Count -ne 1) {
        throw "missing distinct real consumer crate $fixtureName"
    }
}
# Explicit doctests include the foreign-controls/generated-seal/driver-seal
# compile-fail contracts with genuine binding/typed-key positive controls.
& cargo test --manifest-path $twoManifestPath --workspace --locked
if ($LASTEXITCODE -ne 0) { throw 'two-consumer archive runtime tests failed' }
& cargo test --manifest-path $twoManifestPath --workspace --doc --locked
if ($LASTEXITCODE -ne 0) { throw 'two-consumer archive doctest contracts failed' }
foreach ($packageName in $packages) {
    $archive = Join-Path $cargoTarget "package/$packageName-$version.crate"
    Write-Output "archive identity: $packageName $((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash)"
}
Write-Output "two-consumer extracted-archive proof passed: public packages=$($packages -join ',') consumers=$twoRoot test-support=$supportPath"

Write-Output "packaged downstream verification passed: version=$version path=$runRoot"

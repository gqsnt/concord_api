$ErrorActionPreference = 'Stop'

function Run-Step($name, [scriptblock]$body) {
    Write-Host "==> $name"
    & $body
    if ($LASTEXITCODE -ne 0) {
        throw "$name failed with exit code $LASTEXITCODE"
    }
}

function Assert-NoMatch($name, $pattern, [string[]]$paths, [string[]]$extra = @()) {
    Write-Host "==> grep: $name"
    $args = @($pattern) + $paths + $extra
    & rg @args
    $code = $LASTEXITCODE
    if ($code -eq 0) {
        throw "Unexpected matches for $name"
    }
    if ($code -ne 1) {
        throw "rg failed for $name with exit code $code"
    }
}

Run-Step 'cargo fmt --check' { cargo fmt --check }
Run-Step 'cargo test --workspace --all-features' { cargo test --workspace --all-features }
Run-Step 'cargo clippy --workspace --all-targets --all-features -- -D warnings' {
    cargo clippy --workspace --all-targets --all-features -- -D warnings
}
Run-Step 'cargo doc --workspace --no-deps --all-features' { cargo doc --workspace --no-deps --all-features }

Assert-NoMatch 'legacy endpoint execution' 'LegacyEndpoint|execute_decoded_ref_with' @('concord_core', 'concord_macros', 'concord_examples')
Assert-NoMatch 'old auth graph' 'AuthPart|AuthController|AuthChain|OneOfAuth|UseCredential' @('concord_core', 'concord_macros', 'concord_examples')
Assert-NoMatch 'old endpoint part stack' 'RoutePart|PolicyPart|BodyPart|PaginationPart' @('concord_core', 'concord_macros', 'concord_examples')
Assert-NoMatch 'unsupported generated placeholders' '__UnsupportedCustomAuth|CustomAuthPlacement|AuthModePlan|OneOf|AllOf' @('concord_core', 'concord_macros', 'concord_examples')
Assert-NoMatch 'old DSL in normal examples' 'scheme:|host:|use_auth|backoff none|response custom|route\.host' @('concord_examples/src')
Assert-NoMatch 'normal docs/examples importing internal' 'use concord_core::internal' @('concord_examples/src', 'docs')
Assert-NoMatch 'codegen raw AST usage' 'crate::ast|use crate::ast|ClientDef|LayerDef|EndpointDef|AuthBlock|AuthCredentialDecl|RateLimitProfilesBlock|RetryProfilesBlock|CacheProfilesBlock|SchemeLit' @('concord_macros/src/codegen')
Assert-NoMatch 'production TODO/FIXME' 'TODO|FIXME' @('concord_core', 'concord_macros', 'concord_examples/src')

Write-Host 'v4 final audit passed'

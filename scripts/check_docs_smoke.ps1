[CmdletBinding()]
param(
    [string]$CheckDocsPath = (Join-Path $PSScriptRoot "check_docs.ps1")
)

$ErrorActionPreference = "Stop"
$ResolvedCheckDocsPath = (Resolve-Path -LiteralPath $CheckDocsPath).Path
$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("synapse-check-docs-smoke-" + [System.Guid]::NewGuid().ToString("N"))

function New-TextFile {
    param(
        [string]$Path,
        [string]$Text
    )

    $Parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $Parent | Out-Null
    Set-Content -LiteralPath $Path -Value $Text -NoNewline
}

try {
    New-Item -ItemType Directory -Force -Path $TempRoot | Out-Null

    New-TextFile (Join-Path $TempRoot "docs/computergames/05_mcp_tool_surface.md") @'
# 05 - MCP Tool Surface

## 1. Design rules

Smoke fixture.

## 2. Tool registry summary

| # | Tool | Verb | Side effect |
|---|---|---|---|
| 1 | `health` | read | none |

## 3. Tool detail

### 3.12 `act_type`

```json
{"dynamics": {"default": "natural"}}
```

### 3.13 `act_press`
'@

    New-TextFile (Join-Path $TempRoot "docs/computergames/06_data_schemas.md") @'
# 06 - Data Schemas

## 1. Core enums

Smoke fixture.

## 8. Error codes (full catalog)

### 8.1 Smoke

```
TOOL_INTERNAL_ERROR
```
'@

    New-TextFile (Join-Path $TempRoot "docs/computergames/07_storage_and_profiles.md") @'
# 07 - Storage and Profiles

## 1. Storage philosophy

Smoke fixture.

## 4. Column families

| CF | Key |
|---|---|
| `CF_EVENTS` | `[seq]` |
'@

    New-TextFile (Join-Path $TempRoot "crates/synapse-core/src/error_codes.rs") @'
pub const TOOL_INTERNAL_ERROR: &str = "TOOL_INTERNAL_ERROR";
'@

    New-TextFile (Join-Path $TempRoot "crates/synapse-storage/src/cf.rs") @'
pub const CF_EVENTS: &str = "events";
pub const CF_SYNTHETIC_MISSING: &str = "synthetic_missing";
'@

    New-TextFile (Join-Path $TempRoot "crates/synapse-mcp/src/server.rs") @'
impl SynapseService {
    #[tool(description = "Return server health")]
    pub async fn health(&self) {}
}
'@

    Write-Host "source_of_truth=check_docs_smoke before_fixture=$TempRoot expected_missing=CF_SYNTHETIC_MISSING"
    $Output = & pwsh -NoProfile -ExecutionPolicy Bypass -File $ResolvedCheckDocsPath -Root $TempRoot -CheckAnchors 2>&1
    $ExitCode = $LASTEXITCODE
    Write-Host "source_of_truth=check_docs_smoke after_exit=$ExitCode"
    Write-Host ($Output | Out-String)

    if ($ExitCode -eq 0) {
        throw "check_docs smoke expected non-zero exit for synthetic CF mismatch"
    }
    if (($Output | Out-String) -notmatch "CF_SYNTHETIC_MISSING") {
        throw "check_docs smoke expected output to name CF_SYNTHETIC_MISSING"
    }
} finally {
    if (Test-Path -LiteralPath $TempRoot) {
        Remove-Item -LiteralPath $TempRoot -Recurse -Force
    }
}

Write-Host "check_docs_smoke: ok"

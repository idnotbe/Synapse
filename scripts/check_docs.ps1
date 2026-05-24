[CmdletBinding()]
param(
    [string]$Root = (Get-Location).Path,
    [switch]$CheckAnchors
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path -LiteralPath $Root).Path
$Failures = [System.Collections.Generic.List[string]]::new()
$AnchorCache = @{}

function Test-ExternalLink {
    param([string]$Link)
    return $Link -match '^[a-z][a-z0-9+.-]*:'
}

function ConvertTo-GithubAnchor {
    param([string]$Heading)
    $Anchor = $Heading.ToLowerInvariant()
    $Anchor = $Anchor -replace '<[^>]+>', ''
    $Anchor = $Anchor -replace '`', ''
    $Anchor = $Anchor -replace '[^\p{L}\p{Nd} _-]', ''
    $Anchor = $Anchor.Trim() -replace '\s+', '-'
    return $Anchor
}

function Get-HeadingAnchors {
    param([string]$Path)
    if ($AnchorCache.ContainsKey($Path)) {
        return $AnchorCache[$Path]
    }

    $Anchors = @{}
    $Lines = Get-Content -LiteralPath $Path
    foreach ($Line in $Lines) {
        if ($Line -match '^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$') {
            $Anchor = ConvertTo-GithubAnchor $Matches[1]
            if ($Anchor.Length -gt 0) {
                $Anchors[$Anchor] = $true
            }
        }
    }
    $AnchorCache[$Path] = $Anchors
    return $Anchors
}

function Get-RelativePathCompat {
    param(
        [string]$Base,
        [string]$Target
    )

    if ([System.IO.Path].GetMethod("GetRelativePath", [type[]]@([string], [string]))) {
        return [System.IO.Path]::GetRelativePath($Base, $Target)
    }

    $BasePath = [System.IO.Path]::GetFullPath($Base)
    if (-not ($BasePath.EndsWith([System.IO.Path]::DirectorySeparatorChar) -or $BasePath.EndsWith([System.IO.Path]::AltDirectorySeparatorChar))) {
        $BasePath = $BasePath + [System.IO.Path]::DirectorySeparatorChar
    }
    $TargetPath = [System.IO.Path]::GetFullPath($Target)
    $BaseUri = [System.Uri]::new($BasePath)
    $TargetUri = [System.Uri]::new($TargetPath)
    return [System.Uri]::UnescapeDataString($BaseUri.MakeRelativeUri($TargetUri).ToString()).Replace('/', [System.IO.Path]::DirectorySeparatorChar)
}

function Test-ActTypeDynamicsDefault {
    param([string]$RepoRoot)

    $ToolSurfacePath = Join-Path $RepoRoot "docs/computergames/05_mcp_tool_surface.md"
    if (-not (Test-Path -LiteralPath $ToolSurfacePath -PathType Leaf)) {
        $Failures.Add("docs/computergames/05_mcp_tool_surface.md: missing MCP tool surface doc")
        return
    }

    $Text = Get-Content -LiteralPath $ToolSurfacePath -Raw
    $BlockMatch = [regex]::Match($Text, '(?s)### 3\.12 `act_type`.*?### 3\.13 `act_press`')
    if (-not $BlockMatch.Success) {
        $Failures.Add("docs/computergames/05_mcp_tool_surface.md: act_type schema block missing")
        return
    }

    $Block = $BlockMatch.Value
    if ($Block -match '"dynamics"\s*:\s*\{[^}]*"default"\s*:\s*"burst"') {
        $Failures.Add('docs/computergames/05_mcp_tool_surface.md: act_type.dynamics default regressed to "burst"; expected "natural"')
    }
    if ($Block -notmatch '"dynamics"\s*:\s*\{[^}]*"default"\s*:\s*"natural"') {
        $Failures.Add('docs/computergames/05_mcp_tool_surface.md: act_type.dynamics default must be "natural"')
    }
}

function Get-RepoText {
    param([string]$RelativePath)

    $Path = Join-Path $RepoRoot $RelativePath
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }
    return Get-Content -LiteralPath $Path -Raw
}

function Get-MarkdownNumberedSection {
    param(
        [string]$Text,
        [string]$Number
    )

    if ([string]::IsNullOrWhiteSpace($Text)) {
        return ""
    }

    $Pattern = "(?ms)^##\s+$([regex]::Escape($Number))\..*?(?=^##\s+\d+\.|\z)"
    $Match = [regex]::Match($Text, $Pattern)
    if (-not $Match.Success) {
        return ""
    }
    return $Match.Value
}

function Get-UniqueValues {
    param([string[]]$Values)

    return @($Values | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique)
}

function Add-MissingValuesFailure {
    param(
        [string]$Label,
        [string]$MissingLabel,
        [string[]]$Actual,
        [string[]]$Allowed
    )

    $ActualValues = Get-UniqueValues $Actual
    $AllowedValues = Get-UniqueValues $Allowed
    $Missing = @($ActualValues | Where-Object { $AllowedValues -notcontains $_ })
    if ($Missing.Count -gt 0) {
        $Failures.Add("${Label}: ${MissingLabel}: $($Missing -join ', ')")
    }
}

function Get-RegexGroupValues {
    param(
        [string]$Text,
        [string]$Pattern
    )

    if ([string]::IsNullOrWhiteSpace($Text)) {
        return @()
    }

    $Values = foreach ($Match in [regex]::Matches($Text, $Pattern)) {
        $Match.Groups[1].Value
    }
    return Get-UniqueValues $Values
}

function Test-M3ColumnFamilyDocs {
    $DocText = Get-RepoText "docs/computergames/07_storage_and_profiles.md"
    if ($null -eq $DocText) {
        $Failures.Add("docs/computergames/07_storage_and_profiles.md: missing storage/profile doc")
        return
    }

    $DocSection = Get-MarkdownNumberedSection $DocText "4"
    $DocCfs = Get-RegexGroupValues $DocSection '`(CF_[A-Z0-9_]+)`'
    $CfSourcePath = Join-Path $RepoRoot "crates/synapse-storage/src/cf.rs"
    if (-not (Test-Path -LiteralPath $CfSourcePath -PathType Leaf)) {
        return
    }

    $CfSource = Get-Content -LiteralPath $CfSourcePath -Raw
    $CodeCfs = Get-RegexGroupValues $CfSource '(?m)^\s*pub\s+const\s+(CF_[A-Z0-9_]+)\s*:'
    if ($CodeCfs.Count -eq 0) {
        return
    }

    Add-MissingValuesFailure "docs/computergames/07_storage_and_profiles.md section 4" "CF constants missing from docs" $CodeCfs $DocCfs
    Add-MissingValuesFailure "crates/synapse-storage/src/cf.rs" "documented CFs missing from code" $DocCfs $CodeCfs
}

function Test-M3McpToolDocs {
    $DocText = Get-RepoText "docs/computergames/05_mcp_tool_surface.md"
    if ($null -eq $DocText) {
        $Failures.Add("docs/computergames/05_mcp_tool_surface.md: missing MCP tool surface doc")
        return
    }

    $DocSection = Get-MarkdownNumberedSection $DocText "2"
    $DocTools = Get-RegexGroupValues $DocSection '\|\s*\d+\s*\|\s*`([a-z][a-z0-9_]*)`'
    $ServerText = Get-RepoText "crates/synapse-mcp/src/server.rs"
    if ($null -eq $ServerText) {
        return
    }

    $RegisteredTools = Get-RegexGroupValues $ServerText '(?ms)#\[tool.*?\]\s*pub\s+async\s+fn\s+([a-z][a-z0-9_]*)\s*\('
    Add-MissingValuesFailure "docs/computergames/05_mcp_tool_surface.md section 2" "registered MCP tools missing from docs" $RegisteredTools $DocTools
}

function Get-CoreErrorCodeConstants {
    $Path = Join-Path $RepoRoot "crates/synapse-core/src/error_codes.rs"
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        $Failures.Add("crates/synapse-core/src/error_codes.rs: missing error code catalog")
        return @()
    }

    $Text = Get-Content -LiteralPath $Path -Raw
    $Names = foreach ($Match in [regex]::Matches($Text, '(?m)^\s*pub\s+const\s+([A-Z][A-Z0-9_]*)\s*:\s*&str\s*=\s*"([A-Z][A-Z0-9_]*)";')) {
        $Name = $Match.Groups[1].Value
        $Value = $Match.Groups[2].Value
        if ($Name -ne $Value) {
            $Failures.Add("crates/synapse-core/src/error_codes.rs: $Name literal is $Value")
        }
        $Name
    }
    return Get-UniqueValues $Names
}

function Get-ReferencedErrorCodes {
    $CratesRoot = Join-Path $RepoRoot "crates"
    if (-not (Test-Path -LiteralPath $CratesRoot -PathType Container)) {
        return @()
    }

    $Names = foreach ($File in Get-ChildItem -LiteralPath $CratesRoot -Recurse -File -Filter "*.rs") {
        if ($File.FullName -match '[\\/](target)[\\/]') {
            continue
        }
        $Text = Get-Content -LiteralPath $File.FullName -Raw
        foreach ($Match in [regex]::Matches($Text, 'error_codes::([A-Z][A-Z0-9_]*)')) {
            $Match.Groups[1].Value
        }
    }
    return Get-UniqueValues $Names
}

function Test-ErrorCodeDocs {
    $DocText = Get-RepoText "docs/computergames/06_data_schemas.md"
    if ($null -eq $DocText) {
        $Failures.Add("docs/computergames/06_data_schemas.md: missing data schema doc")
        return
    }

    $DocSection = Get-MarkdownNumberedSection $DocText "8"
    $DocCodes = Get-RegexGroupValues $DocSection '(?m)^\s*([A-Z][A-Z0-9_]+)\s*$'
    $ConstCodes = Get-CoreErrorCodeConstants
    $ReferencedCodes = Get-ReferencedErrorCodes

    Add-MissingValuesFailure "crates/**/*.rs" "referenced error codes missing pub const declarations" $ReferencedCodes $ConstCodes
    Add-MissingValuesFailure "docs/computergames/06_data_schemas.md section 8" "referenced error codes missing docs entries" $ReferencedCodes $DocCodes
    Add-MissingValuesFailure "docs/computergames/06_data_schemas.md section 8" "pub const error codes missing docs entries" $ConstCodes $DocCodes
}

function Test-BundledProfileDocs {
    $ProfileRoot = Join-Path $RepoRoot "profiles"
    if (-not (Test-Path -LiteralPath $ProfileRoot -PathType Container)) {
        return
    }

    $DocText = Get-RepoText "docs/computergames/07_storage_and_profiles.md"
    if ($null -eq $DocText) {
        $Failures.Add("docs/computergames/07_storage_and_profiles.md: missing storage/profile doc")
        return
    }

    $ProfilePaths = foreach ($File in Get-ChildItem -LiteralPath $ProfileRoot -Recurse -File -Filter "*.toml") {
        (Get-RelativePathCompat $RepoRoot $File.FullName).Replace('\', '/')
    }
    foreach ($ProfilePath in Get-UniqueValues $ProfilePaths) {
        if ($DocText -notmatch [regex]::Escape($ProfilePath)) {
            $Failures.Add("docs/computergames/07_storage_and_profiles.md: bundled profile path missing from docs: $ProfilePath")
        }
    }
}

$Files = [System.Collections.Generic.List[System.IO.FileInfo]]::new()
$RootReadme = Join-Path $RepoRoot "README.md"
if (Test-Path -LiteralPath $RootReadme) {
    $Files.Add((Get-Item -LiteralPath $RootReadme))
}

$DocsRoot = Join-Path $RepoRoot "docs"
if (Test-Path -LiteralPath $DocsRoot) {
    Get-ChildItem -LiteralPath $DocsRoot -Recurse -File -Filter "*.md" | ForEach-Object {
        $Files.Add($_)
    }
}

Test-ActTypeDynamicsDefault $RepoRoot
Test-M3ColumnFamilyDocs
Test-M3McpToolDocs
Test-ErrorCodeDocs
Test-BundledProfileDocs

foreach ($File in $Files) {
    $Lines = Get-Content -LiteralPath $File.FullName
    for ($Index = 0; $Index -lt $Lines.Count; $Index++) {
        $LineNumber = $Index + 1
        $Matches = [regex]::Matches($Lines[$Index], '(?<!\!)\[[^\]]+\]\(([^)]+)\)')
        foreach ($Match in $Matches) {
            $RawLink = $Match.Groups[1].Value.Trim()
            if ($RawLink.StartsWith("<") -and $RawLink.EndsWith(">")) {
                $RawLink = $RawLink.Substring(1, $RawLink.Length - 2)
            }
            if ([string]::IsNullOrWhiteSpace($RawLink) -or (Test-ExternalLink $RawLink)) {
                continue
            }

            $LinkTarget = ($RawLink -split '\s+', 2)[0]
            $Parts = $LinkTarget -split '#', 2
            $PathPart = [System.Uri]::UnescapeDataString($Parts[0])
            $AnchorPart = $null
            if ($Parts.Count -eq 2) {
                $AnchorPart = [System.Uri]::UnescapeDataString($Parts[1]).ToLowerInvariant()
            }

            if ([string]::IsNullOrWhiteSpace($PathPart)) {
                $TargetPath = $File.FullName
            } else {
                $TargetPath = [System.IO.Path]::GetFullPath((Join-Path $File.DirectoryName $PathPart))
            }

            $RelativeFile = Get-RelativePathCompat $RepoRoot $File.FullName
            if (-not (Test-Path -LiteralPath $TargetPath -PathType Leaf)) {
                $Failures.Add("${RelativeFile}:${LineNumber}: broken markdown link '$RawLink' -> '$PathPart'")
                continue
            }

            if ($CheckAnchors -and -not [string]::IsNullOrWhiteSpace($AnchorPart)) {
                $Anchors = Get-HeadingAnchors $TargetPath
                if (-not $Anchors.ContainsKey($AnchorPart)) {
                    $Failures.Add("${RelativeFile}:${LineNumber}: missing anchor '#$AnchorPart' in '$PathPart'")
                }
            }
        }
    }
}

if ($Failures.Count -gt 0) {
    foreach ($Failure in $Failures) {
        Write-Error $Failure -ErrorAction Continue
    }
    exit 1
}

Write-Host "check_docs: ok ($($Files.Count) markdown files)"

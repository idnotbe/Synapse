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

            $RelativeFile = [System.IO.Path]::GetRelativePath($RepoRoot, $File.FullName)
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
        Write-Error $Failure
    }
    exit 1
}

Write-Host "check_docs: ok ($($Files.Count) markdown files)"

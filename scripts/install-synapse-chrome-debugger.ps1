param(
    [string]$SynapseNativeHostExe = "$env:USERPROFILE\.cargo\bin\synapse-chrome-native-host.exe",
    [string]$ExtensionId = "leoocgnkjnplbfdbklajepahofecgfbk"
)

$ErrorActionPreference = 'Stop'
$silentDebuggerSwitch = '--silent-debugger-extension-api'

$repoRoot = Split-Path -Parent $PSScriptRoot
$extensionDir = Join-Path $repoRoot 'extensions\synapse-chrome-debugger'
$manifestPath = Join-Path $extensionDir 'manifest.json'
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "SYNAPSE_CHROME_EXTENSION_MANIFEST_MISSING path=$manifestPath"
}
$extensionManifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$requiredPermissions = @($extensionManifest.permissions)
$optionalPermissions = @($extensionManifest.optional_permissions)
if ($requiredPermissions -contains 'debugger') {
    throw "SYNAPSE_CHROME_EXTENSION_REQUIRED_DEBUGGER_PERMISSION_FORBIDDEN path=$manifestPath remediation=normal end-user bridge must use chrome.tabs without required debugger permission"
}
if ($optionalPermissions -contains 'debugger') {
    throw "SYNAPSE_CHROME_EXTENSION_OPTIONAL_DEBUGGER_PERMISSION_FORBIDDEN path=$manifestPath remediation=Chrome does not allow debugger as optional permission; use a separate debugger-enabled bridge only with --silent-debugger-extension-api"
}
if (-not (Test-Path -LiteralPath $SynapseNativeHostExe -PathType Leaf)) {
    throw "SYNAPSE_CHROME_NATIVE_HOST_BINARY_MISSING path=$SynapseNativeHostExe remediation=build/install synapse-chrome-native-host first"
}

$nativeRoot = Join-Path $env:APPDATA 'synapse\chrome-debugger'
New-Item -ItemType Directory -Force -Path $nativeRoot | Out-Null

$hostName = 'com.synapse.chrome_debugger'
$hostManifestPath = Join-Path $nativeRoot "$hostName.json"
$hostManifest = [ordered]@{
    name = $hostName
    description = 'Synapse Chrome tabs/native-messaging bridge (no-console host)'
    path = (Resolve-Path -LiteralPath $SynapseNativeHostExe).Path
    type = 'stdio'
    allowed_origins = @("chrome-extension://$ExtensionId/")
}
$hostManifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $hostManifestPath -Encoding UTF8

$registryPath = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
$registrySubKey = "Software\Google\Chrome\NativeMessagingHosts\$hostName"
New-Item -Path $registryPath -Force | Out-Null
$registryKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($registrySubKey, $true)
if ($null -eq $registryKey) {
    throw "SYNAPSE_CHROME_NATIVE_HOST_REGISTRY_OPEN_FAILED path=$registryPath"
}
try {
    $registryKey.SetValue('', $hostManifestPath, [Microsoft.Win32.RegistryValueKind]::String)
}
finally {
    $registryKey.Dispose()
}

$readbackManifest = Get-Content -Raw -LiteralPath $hostManifestPath | ConvertFrom-Json
$readbackRegistryKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($registrySubKey, $false)
if ($null -eq $readbackRegistryKey) {
    throw "SYNAPSE_CHROME_NATIVE_HOST_REGISTRY_READ_FAILED path=$registryPath"
}
try {
    $readbackRegistry = $readbackRegistryKey.GetValue('')
}
finally {
    $readbackRegistryKey.Dispose()
}

if ($readbackRegistry -ne $hostManifestPath) {
    throw "SYNAPSE_CHROME_NATIVE_HOST_REGISTRY_MISMATCH expected=$hostManifestPath actual=$readbackRegistry"
}
if ($readbackManifest.name -ne $hostName) {
    throw "SYNAPSE_CHROME_NATIVE_HOST_NAME_MISMATCH expected=$hostName actual=$($readbackManifest.name)"
}
if ($readbackManifest.allowed_origins[0] -ne "chrome-extension://$ExtensionId/") {
    throw "SYNAPSE_CHROME_NATIVE_HOST_ORIGIN_MISMATCH expected=chrome-extension://$ExtensionId/ actual=$($readbackManifest.allowed_origins[0])"
}

$chromeProcesses = @(Get-CimInstance Win32_Process -Filter "Name='chrome.exe'" -ErrorAction SilentlyContinue | ForEach-Object {
    $commandLine = [string]$_.CommandLine
    [pscustomobject]@{
        pid = [int]$_.ProcessId
        command_line_readable = -not [string]::IsNullOrWhiteSpace($commandLine)
        has_silent_debugger_switch = $commandLine -match '(^|\s)--silent-debugger-extension-api(\s|=|$)'
    }
})

[pscustomobject]@{
    ok = $true
    native_host = $hostName
    native_manifest = $hostManifestPath
    registry_key = $registryPath
    binary = $readbackManifest.path
    extension_id = $ExtensionId
    extension_dir = $extensionDir
    background_navigation_backend = 'chrome.tabs_no_debugger_permission'
    attach_popup_prevention = 'normal_bridge_has_no_required_debugger_permission_plus_daemon_preflight_extension_attestation_gate'
    required_debugger_permission_present = $false
    optional_debugger_permission_present = $false
    silent_debugger_switch_required_for_attach_commands = $true
    silent_debugger_switch = $silentDebuggerSwitch
    current_chrome_processes = $chromeProcesses
}

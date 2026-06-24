# Issues 1314 And 1315 Chrome Installer FSV - 2026-06-24

Issues:

- https://github.com/ChrisRoyse/Synapse/issues/1314
- https://github.com/ChrisRoyse/Synapse/issues/1315

This transcript records the full-state verification for the Chrome bridge
installer hardening work. The run used the real installed Chrome profile, the
real Synapse setup script, and same-process MCP readback after daemon restart.

## Code Surface

The installer now fails closed when the HKCU `ExtensionSettings` popup shield
cannot be written:

- Blocking code: `SYNAPSE_CHROME_POLICY_POPUP_SHIELD_WRITE_DENIED_BLOCKING`
- Detail includes the exact blocking policy rows and ACL/readback diagnostics
  returned by `Set-SynapseChromeExternalDebuggerPolicy`.
- Remediation tells the operator to repair
  `HKCU\Software\Policies\Google\Chrome` ACL or run from an elevated maintenance
  PowerShell.
- Setup no longer accepts a soft warning when the reversible self-shield cannot
  be verified.

The first-install UIA path now filters Chrome windows before driving
`chrome://extensions`:

- A Chrome process with no `--user-data-dir` is treated as the normal default
  user-data root.
- A Chrome process with `--user-data-dir` must match the active profile's user
  data root exactly.
- `ms-playwright-mcp`, unreadable command lines, and other dedicated
  user-data-dir Chrome processes are rejected.
- The `Load unpacked` probe only searches the originally selected window after
  its title confirms `Extensions - Google Chrome`.
- The folder picker must belong to the selected Chrome process PID.

## Installer Readback

The real installer was run directly:

```powershell
.\scripts\install-synapse-chrome-debugger.ps1 `
  -SynapseNativeHostExe "$env:USERPROFILE\.cargo\bin\synapse-chrome-native-host.exe" `
  -AutoInstallTimeoutSeconds 30
```

Result:

- `ok=true`
- Extension ID: `leoocgnkjnplbfdbklajepahofecgfbk`
- Auto install attempted: `true`
- Auto install changed: `false`
- Auto install reason:
  `existing_ready_extension_code_reload_deferred_to_daemon_reloadself`
- Active profile: `Profile 5`
- Required foreground: `false`
- Manifest path:
  `C:\Users\hotra\AppData\Local\synapse\chrome-extension\synapse-chrome-bridge-2026-06-24-mousedown-click-v3`
- Manifest path matches stable path: `true`
- Manifest dir exists: `true`
- Missing active API permissions: `[]`
- Disable reasons: `[]`
- Enabled and permissioned: `true`

Policy shield readback:

- Hive: `HKCU`
- Path: `HKCU:\Software\Policies\Google\Chrome`
- Changed: `true`
- Reason: `synapse_authored_popup_shield_applied`
- Shielded self entry: `leoocgnkjnplbfdbklajepahofecgfbk`
- Preserved existing external hazard entries:
  `fcoeoabgfenejglbffodgkkbkcdhcgfn`,
  `inomeogfingihgjfjlpeplalcfajhgai`

Profile install state:

- Chrome user data root:
  `C:\Users\hotra\AppData\Local\Google\Chrome\User Data`
- Profile count: `7`
- Installed profile count: `1`
- Installed profile: `Profile 5`
- Active profile installed: `true`
- Reason: `extension_profile_row_present`
- `cdp_bridge_reload_can_install_absent_extension=false`

## Selector Readback

The installer function definitions were loaded from the script AST without
running the top-level installer, then
`Get-SynapseChromeProcessProfileMatch` was exercised with synthetic command
lines:

- `chrome.exe`: eligible, reason `default_chrome_user_data_root`
- `chrome.exe --user-data-dir="<normal Chrome User Data>" --profile-directory="Profile 5"`:
  eligible, reason `user_data_root_matches_active_profile_root`
- `chrome.exe --user-data-dir="<LOCALAPPDATA>\ms-playwright-mcp\profile" --remote-debugging-pipe --disable-blink-features=AutomationControlled`:
  rejected, reason `dedicated_ms_playwright_mcp_user_data_dir`
- `chrome.exe --user-data-dir="C:\Temp\other-chrome" --remote-debugging-port=9222`:
  rejected, reason `user_data_root_mismatch`

Live UIA window enumeration against the real machine returned three eligible
normal Chrome windows, all from PID `12956` with no `--user-data-dir` override:

- `Synapse Command Center - Google Chrome`
- `Extensions - Google Chrome`
- `Synapse Browser PDF FSV - Google Chrome`

The live readback did not expose an eligible `ms-playwright-mcp` or remote
debugging Chrome window to the first-install selector.

## Setup Handoff

Full setup was run in skip-build mode because this change only modified the
installer script:

```powershell
.\scripts\synapse-setup.ps1 -SkipBuild -ForceRestart
```

Setup readback:

- Candidate binary:
  `C:\Users\hotra\.cargo\bin\synapse-mcp.exe`
- Candidate SHA256:
  `23819CF7D07024DF6628BF1A479F3F3F2197B0B732279D47C659AFF0754A58B8`
- Candidate tool count: `166`
- Candidate tool surface SHA256:
  `d337749baf7698d1c0af119be4bf795317e47aaf1af0f4c4741bf0fb7abe33b6`
- Chrome bridge preflight popup shield:
  `HKCU:synapse_authored_popup_shield_applied`
- Chrome bridge preflight active profile: `Profile 5`
- Chrome bridge preflight active profile installed: `true`
- Post-copy Chrome bridge verifier popup shield:
  `HKCU:synapse_authored_popup_shield_applied`
- Post-copy active profile installed: `true`
- New daemon PID: `30412`
- Setup health: daemon OK on `127.0.0.1:7700`
- Chrome bridge after daemon start: `stale=false`, capability
  `pageScreenshot`
- Codex tool-surface snapshot updated with tool count `166`

The setup output reported the current Codex process schema as stale but
nonfatal. Same-process MCP calls below verified reconnect without opening a new
Codex terminal.

## Same-Process MCP Readback

`mcp__synapse.health` from this same Codex session after setup:

- `ok=true`
- PID: `30412`
- Tool count: `166`
- Tool surface SHA256:
  `d337749baf7698d1c0af119be4bf795317e47aaf1af0f4c4741bf0fb7abe33b6`
- Chrome bridge status: `ok`
- Extension stale: `false`
- Extension build:
  `synapse-chrome-bridge-2026-06-24-mousedown-click-v3`
- Bridge popup risk suppression:
  `status=clear`, `remaining_hazard_count=0`, `failure_count=0`
- Self policy shield:
  `synapse_chrome_self_policy_shield_present=true`
- Policy write access:
  `policy_set_value_access=true`
- Active installed profile: `Profile 5`
- Layout infobar risk warning: `false`

`mcp__synapse.tool_profile_status`:

- Implementation tool count: `216`
- Visible normal-profile tool count: `166`
- Visible hash:
  `sha256:0fb20e1cc24fe51ff28bab1b8dc36bef22fb4b164731d4f965b9c1257e87cd53`
- Policy row hash matched the visible hash.

Session and control readback:

- `session_list live_only`: one live Codex session.
- `control_lease_status`: `held=false`.
- `target_claim_status`: `claim_count=0`.

## Acceptance Mapping

- #1314 policy shield denial: PASS. A denied policy write is now a blocking
  installer error with ACL/remediation detail instead of a soft warning. The
  live machine also verified successful HKCU policy shield write access and a
  present Synapse self-shield.
- #1315 wrong Chrome selector: PASS. The selector rejects `ms-playwright-mcp`
  and other dedicated `--user-data-dir` Chrome processes, selects only the
  active-profile user-data root or default-root Chrome, confirms the selected
  window reached `chrome://extensions`, and binds the folder picker to the
  selected Chrome PID.
- Reconnect guarantee: PASS. `synapse-setup.ps1` restarted the daemon and this
  same Codex process immediately read `health`, `tool_profile_status`,
  `session_list`, `control_lease_status`, and `target_claim_status` from the new
  daemon.

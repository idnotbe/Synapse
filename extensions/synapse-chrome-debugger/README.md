# Synapse Chrome Bridge

This unpacked MV3 extension lets the Synapse daemon inspect and control the
user's normal Chrome profile through Chrome Native Messaging. The normal
end-user bridge is tabs-first: background tab open/close/navigation use
`chrome.tabs` APIs and the extension does not require the `debugger` permission.

Stable extension ID: `leoocgnkjnplbfdbklajepahofecgfbk`

Native host name: `com.synapse.chrome_debugger`

Install the native host registration with:

```powershell
scripts\install-synapse-chrome-debugger.ps1
```

Then load this directory as an unpacked extension from `chrome://extensions`.
The extension keeps one `runtime.connectNative()` port open and executes
`chrome.tabs` commands only after the daemon asks through the local
authenticated bridge.

Background tab commands (`openTab`, `closeTab`, and `navigateTab`) use
`chrome.tabs.create`, `chrome.tabs.remove`, `chrome.tabs.update`,
`chrome.tabs.reload`, `chrome.tabs.goBack`, and `chrome.tabs.goForward`. They do
not call `chrome.debugger.getTargets` or `chrome.debugger.attach`; target IDs
returned by this path are synthetic `chrome-tab:<tabId>` IDs backed by
`chrome.tabs` readback.

Attach-capable commands (`snapshot`, `clickNode`, `typeNode`, and `nodeValue`)
are unavailable in the normal end-user install unless a separate
debugger-enabled path is explicitly configured. Without
`--silent-debugger-extension-api`, Chrome intentionally shows its "`started
debugging this browser`" warning UI when an extension calls
`chrome.debugger.attach`. Synapse checks the target window owner PID and process
command line before attach; if the switch is absent or unreadable, Synapse
returns `A11Y_CDP_DEBUGGER_WARNING_UNSUPPRESSED` and does not call
`chrome.debugger.attach`. The extension also requires the daemon's explicit
suppression attestation on attach-capable native commands, so stale or malformed
native commands fail before `chrome.debugger.attach`.

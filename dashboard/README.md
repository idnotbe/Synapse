# Synapse Command Center Dashboard

Local-only browser dashboard for the Synapse daemon.

## Build

```powershell
bun install --frozen-lockfile
bun run build
```

The build writes committed, hashed static assets to `dashboard/dist/`. The Rust daemon embeds those
files and serves them on loopback under `/dashboard`; Bun, Vite, and Node-compatible tooling are
build-time only and are not part of the runtime.

## Local Checks

```powershell
bun run check
```

The check is a supporting charter lint. Manual Synapse FSV remains the acceptance gate.

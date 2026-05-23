# Changelog

## v0.1.0-m0 - 2026-05-23

M0 bootstraps Synapse as a Rust MCP server with a single `health` tool over stdio.

- Added the Rust workspace, crate skeletons, dual license files, cargo-deny configuration, CI workflow, and helper scripts.
- Implemented `synapse-core` shared types, schema version, and M0 error-code constants.
- Implemented `synapse-telemetry` JSON file logging, console logging, and log-dir validation.
- Implemented `synapse-mcp` stdio startup, CLI flags, graceful shutdown logging, and the `health` MCP tool.
- Implemented `synapse-test-utils` raw stdio JSON-RPC client and M0 end-to-end health demo tests.
- Added root README quick start, current Rust/dependency ADR, documentation link checking, and WSL-global Codex/Claude Code MCP configuration guidance.

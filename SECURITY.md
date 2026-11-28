# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.0.x   | ✅ Yes    |

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.**

Instead, email the maintainer at `kubuntu@example.com` with:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

You will receive a response within 7 days. If the vulnerability is confirmed, a fix will be released as soon as possible.

## Security Considerations

### Local-First Architecture

lai is designed to run entirely locally:

- All LLM inference happens on your machine via llama.cpp or Ollama
- No code or data leaves your network unless you explicitly configure a cloud endpoint
- The `is_local_url()` guard prevents accidental use of public APIs (Regla 4)

### Known Attack Surface

| Component | Risk | Mitigation |
|-----------|------|------------|
| `run_command` / `bash` / `run_python` | Arbitrary code execution | Sandbox opt-in (`"sandbox": true`) or mandatory (`lai --sandbox` / `LAI_REQUIRE_SANDBOX=1`); fail-closed if bubblewrap is unavailable. Without the sandbox, only run `auto` on trusted repos |
| `apply_patch` / `write_file` / `edit` | Overwrite arbitrary files | Workspace boundary in `PolicyEngine`, anti-destruction guard, manifest guard (`allow_manifest_overwrite` to opt out) |
| Gateway (port 8081) | Unauthenticated access | Binds to `127.0.0.1` by default; no public exposure |
| llama.cpp (port 8080) | Model extraction / DoS | Loopback only; no auth |
| Prompt injection from repo files | Model may issue harmful tool calls | Sandbox + `LAI_COMMAND_ALLOWLIST=1` (opt-in) + trusted-repo policy |

### Sandbox

The Rust CLI can execute shell and Python inside bubblewrap:

```bash
lai --sandbox auto "task"            # mandatory, fail-closed
LAI_REQUIRE_SANDBOX=1 lai auto "…"   # same via environment
```

- Workspace is read/write; the rest of the system is read-only; `/tmp` is
  writable; capabilities are dropped; the network is **blocked** by default.
- `LAI_SANDBOX_ALLOW_NET=1` or `"sandbox_network": true` re-shares the host
  network while keeping PID/mount/IPC isolation (needed by tasks that download
  dependencies).
- The sandbox is Linux/Unix only. If `bwrap` is missing, `--sandbox` fails
  instead of silently running unisolated.

**Manifest guard:** `write_file`, `edit`, `apply_patch` and obvious shell
redirections (`> Cargo.toml`, `rm`, `tee`, `dd of=…`) are refused unless
`allow_manifest_overwrite: true`. The shell guard is best-effort: obfuscation
(variables, `python -c`, nested shells) can bypass it; the sandbox is the
control that does not depend on parsing the command.

**Command allowlist:** `LAI_COMMAND_ALLOWLIST=1` routes `run_command`, `bash`
and `run_python` through the `PolicyEngine` allowlist (extend it with
`LAI_ALLOW_COMMANDS=make,sed,…`). It is off by default because coding agents
need many commands; enable it for untrusted repositories.

### Hardening Recommendations

If you expose lai beyond loopback:

1. Put gateway behind a reverse proxy with auth (TLS + Basic/OAuth)
2. Enable `rate_limit.enabled: true` in `gateway-go/config.yaml`
3. Set `server.max_request_size` lower
4. Run llama.cpp with `--mlock` to prevent swapping model weights
5. Use a dedicated user account for lai services

### Dependencies

- `requirements.txt` / `requirements-dev.txt` pinned to safe versions
- Go dependencies in `gateway-go/go.mod` — run `go list -m -json all` to audit
- No runtime code download or eval from untrusted sources

## Vulnerability Disclosure Timeline

1. **Day 0**: Report received
2. **Day 1-3**: Triage and confirmation
3. **Day 4-14**: Fix development and testing
4. **Day 15**: Release patch version
5. **Day 16+**: Public disclosure (CVE if applicable)

## Scope

This policy covers:
- `lai.py` and all Python modules
- `gateway-go/` (Go gateway)
- `scripts/` (startup and utility scripts)
- Configuration files

It does **not** cover:
- llama.cpp / Ollama upstream vulnerabilities (report to their projects)
- Model weights (Qwen3-8B, etc.)
- Hardware/firmware issues
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
| `run_command` / `python_eval` | Arbitrary code execution | Only enable AutoMode on trusted repos; not a sandbox |
| Gateway (port 8081) | Unauthenticated access | Binds to `127.0.0.1` by default; no public exposure |
| llama.cpp (port 8080) | Model extraction / DoS | Loopback only; no auth |
| File write tools | Overwrite arbitrary files | Validates Python syntax; user confirms in CLI |

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
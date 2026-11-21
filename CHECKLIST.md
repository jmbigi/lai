# lai Release Checklist

Use this checklist before tagging a new version.

## Pre-Release

- [ ] All tests pass: `make test`
- [ ] Linting clean: `make lint`
- [ ] Type checking clean: `make type`
- [ ] Formatting applied: `make format`
- [ ] Full check suite: `make check`
- [ ] `python3 -m py_compile lai.py` succeeds

## Functionality

- [ ] `python3 lai.py status` — all services detected
- [ ] `python3 lai.py tools` — lists all tools
- [ ] `python3 lai.py review fib.py` — produces review
- [ ] `python3 lai.py auto "list files" --max-iterations 2` — runs AutoMode
- [ ] `python3 lai.py serve --port 8082` — health, models, chat work
- [ ] Gateway builds: `make gateway-build`
- [ ] Gateway health: `curl http://127.0.0.1:8081/health`
- [ ] Gateway tools: tool calling works through gateway
- [ ] Streaming: `curl -N` shows SSE chunks with reasoning_content

## Documentation

- [ ] `README.md` updated with new features/commands
- [ ] `CHANGELOG.md` entry added (see format below)
- [ ] `DOCUMENTACION_REGLAS.md` reflects current rules
- [ ] `AGENTS.md` updated if architecture changed
- [ ] Version bumped in:
  - [ ] `VERSION` file
  - [ ] `pyproject.toml` `[project].version`
  - [ ] `gateway-go/config.yaml` (if gateway changed)
  - [ ] `gateway-go/main.go` log line (if gateway changed)

## Configuration

- [ ] `config.yaml` ports: llama=8080, gateway=8081, serve=8082
- [ ] Context window: 32768 everywhere
- [ ] Reasoning format: `deepseek` + `reasoning-budget`
- [ ] Bind addresses: `127.0.0.1` for gateway and serve

## Git Hygiene

- [ ] No `__pycache__`, `*.pyc`, `.pytest_cache` in commit
- [ ] No `gateway-go/gateway` binary in commit (built on CI)
- [ ] No `sessions/*.json` in commit
- [ ] `.gitignore` covers all generated files
- [ ] Commit message follows conventional format

## Post-Release

- [ ] Tag created: `git tag v0.0.x`
- [ ] Tag pushed: `git push origin v0.0.x`
- [ ] GitHub Release created with changelog
- [ ] Docker image built (if applicable)
- [ ] Announcement prepared

---

## CHANGELOG.md Entry Format

```markdown
## [v0.0.x] - YYYY-MM-DD

### Added
- New feature description

### Changed
- Modified behavior description

### Fixed
- Bug fix description

### Removed
- Deprecated feature removal
```

## Version Bumping

| Change Type | Version Bump |
|-------------|--------------|
| Bug fix | PATCH (0.0.x → 0.0.x+1) |
| New feature, non-breaking | MINOR (0.x.0 → 0.x+1.0) |
| Breaking change | MAJOR (x.0.0 → x+1.0.0) |

Current version: `v0.0.2`
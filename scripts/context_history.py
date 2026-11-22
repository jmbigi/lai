"""Registro de contexto e historial de sesiones para lai.

100% local: almacena en archivos JSON en ~/.local-ai/
Sin cloud, sin API keys, sin cuentas externas.
Permite registrar prompts, resultados, reglas aplicadas y recuperar
contexto de sesiones previas para dar continuidad al trabajo.
"""

from __future__ import annotations

import json
import os
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


STORAGE_DIR = Path(os.environ.get("LAI_STORAGE_DIR", Path.home() / ".local-ai"))
SESSIONS_FILE = STORAGE_DIR / "sessions.json"
HISTORY_FILE = STORAGE_DIR / "history.json"
CORPUS_FILE = STORAGE_DIR / "corpus.json"


def _ensure_storage() -> None:
    STORAGE_DIR.mkdir(parents=True, exist_ok=True)
    if not SESSIONS_FILE.exists():
        SESSIONS_FILE.write_text("[]", encoding="utf-8")
    if not HISTORY_FILE.exists():
        HISTORY_FILE.write_text("[]", encoding="utf-8")
    if not CORPUS_FILE.exists():
        CORPUS_FILE.write_text("{}", encoding="utf-8")


def _load_json(path: Path) -> list[Any]:
    _ensure_storage()
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, FileNotFoundError):
        return []


def _save_json(path: Path, data: list[Any] | dict[str, Any]) -> None:
    _ensure_storage()
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")


def start_session(
    model: str,
    base_url: str,
    action: str,
    rules: list[str] | None = None,
) -> str:
    """Inicia una nueva sesion y retorna su ID."""
    _ensure_storage()
    sessions = _load_json(SESSIONS_FILE)
    session_id = datetime.now(timezone.utc).strftime("%Y%m%d%H%M%S")
    session: dict[str, Any] = {
        "id": session_id,
        "started_at": datetime.now(timezone.utc).isoformat(),
        "model": model,
        "base_url": base_url,
        "action": action,
        "rules": rules or [],
        "files_processed": 0,
        "total_issues": 0,
        "completed": False,
    }
    sessions.append(session)
    _save_json(SESSIONS_FILE, sessions)
    return session_id


def end_session(session_id: str) -> None:
    """Marca una sesion como completada."""
    sessions = _load_json(SESSIONS_FILE)
    for s in sessions:
        if s["id"] == session_id:
            s["completed"] = True
            s["ended_at"] = datetime.now(timezone.utc).isoformat()
            break
    _save_json(SESSIONS_FILE, sessions)


def record_entry(
    session_id: str,
    file_path: str,
    rule: str,
    message: str,
    line: int | None = None,
    severity: str | None = None,
) -> None:
    """Registra una entrada individual en el historial."""
    _ensure_storage()
    history = _load_json(HISTORY_FILE)
    entry: dict[str, Any] = {
        "session_id": session_id,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "file_path": file_path,
        "rule": rule,
        "message": message,
        "line": line,
        "severity": severity,
    }
    history.append(entry)
    _save_json(HISTORY_FILE, history)


def update_session_stats(
    session_id: str,
    files_processed: int,
    total_issues: int,
) -> None:
    """Actualiza contadores de una sesion."""
    sessions = _load_json(SESSIONS_FILE)
    for s in sessions:
        if s["id"] == session_id:
            s["files_processed"] = files_processed
            s["total_issues"] = total_issues
            break
    _save_json(SESSIONS_FILE, sessions)


def save_corpus(project_id: str, corpus: dict[str, Any]) -> None:
    """Guarda el corpus comprimido de un proyecto para reutilizacion."""
    _ensure_storage()
    corpora = _load_corpus_map()
    corpora[project_id] = {
        **corpus,
        "updated_at": datetime.now(timezone.utc).isoformat(),
    }
    _save_json(CORPUS_FILE, corpora)


def load_corpus(project_id: str) -> dict[str, Any] | None:
    """Carga el corpus comprimido de un proyecto."""
    _ensure_storage()
    corpora = _load_corpus_map()
    return corpora.get(project_id)


def _load_corpus_map() -> dict[str, Any]:
    try:
        return json.loads(CORPUS_FILE.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, FileNotFoundError):
        return {}


def get_history(limit: int = 50) -> list[Any]:
    """Retorna las últimas entradas del historial."""
    history = _load_json(HISTORY_FILE)
    return history[-limit:]


def get_sessions(limit: int = 20) -> list[Any]:
    """Retorna las últimas sesiones."""
    sessions = _load_json(SESSIONS_FILE)
    return sessions[-limit:]


def get_context_for_session(session_id: str) -> dict[str, Any]:
    """Retorna el contexto completo de una sesion para continuidad."""
    sessions = _load_json(SESSIONS_FILE)
    session = next((s for s in sessions if s["id"] == session_id), None)
    history = _load_json(HISTORY_FILE)
    entries = [e for e in history if e["session_id"] == session_id]
    return {
        "session": session,
        "entries": entries,
    }

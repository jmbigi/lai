#!/usr/bin/env python3
"""Diagnóstico del entorno Docker, compartido por las etapas 2–4.

Motivación real (2026-11-26): en este equipo el CLI de Docker se quedaba
colgado 30–60 s por llamada y todo el pipeline lo interpretaba como «Docker no
está disponible», sin decir por qué. La causa era Docker Desktop 4.22 con
WSL 3.0.5: versiones incompatibles. Un mensaje claro ahorra una hora de
diagnóstico, así que la comprobación devuelve la causa y una pista accionable.

Uso:
    from docker_env import check
    if not check():
        raise SystemExit(2)

No importa nada fuera de la stdlib y no necesita el daemon para los tests.
"""

from __future__ import annotations

import subprocess

#: Versión mínima de servidor que el harness da por buena. No es un requisito
#: técnico estricto: es un aviso, porque servidores muy antiguos suelen venir
#: de un Docker Desktop desactualizado frente a WSL.
MIN_SERVER_MAJOR = 20

HINTS = (
    "  • Comprueba que Docker Desktop está abierto y terminó de arrancar.",
    "  • Si el CLI se queda colgado, reinicia el motor: `wsl --shutdown` y "
    "vuelve a abrir Docker Desktop.",
    "  • Si la versión del servidor es muy antigua para tu WSL, actualiza: "
    "`winget upgrade --id Docker.DockerDesktop`.",
    "  • `docker version` muestra por separado la versión de cliente y servidor; "
    "si el servidor no aparece, el problema es el motor, no los scripts.",
)


def server_version(timeout: int = 30) -> tuple[str | None, str]:
    """Devuelve (versión, "") o (None, motivo legible)."""
    try:
        result = subprocess.run(
            ["docker", "info", "--format", "{{.ServerVersion}}"],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except FileNotFoundError:
        return None, "el comando 'docker' no existe en PATH"
    except subprocess.TimeoutExpired:
        return None, f"el motor no respondió en {timeout}s (CLI colgado o Docker Desktop arrancando)"
    except OSError as exc:
        return None, f"no se pudo ejecutar 'docker': {exc}"
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        return None, detail[:300] or f"'docker info' salió con código {result.returncode}"
    version = (result.stdout or "").strip()
    if not version:
        return None, "'docker info' no devolvió ninguna versión de servidor"
    return version, ""


def major_of(version: str) -> int | None:
    """Extrae el número mayor de una versión tipo '29.9.3' o '24.0.5'."""
    head = version.split(".", 1)[0].strip()
    return int(head) if head.isdigit() else None


def check(timeout: int = 30, quiet: bool = False) -> bool:
    """True si hay motor Docker usable. Si no, explica el motivo y las pistas."""
    version, problem = server_version(timeout)
    if version is None:
        if not quiet:
            print(f"❌ Docker no está disponible: {problem}", flush=True)
            for line in HINTS:
                print(line, flush=True)
        return False

    major = major_of(version)
    if major is not None and major < MIN_SERVER_MAJOR and not quiet:
        print(
            f"⚠ Servidor Docker {version} es antiguo (se esperaba ≥ {MIN_SERVER_MAJOR}.x). "
            "El harness puede funcionar, pero si el CLI va lento o se cuelga, "
            "actualiza Docker Desktop.",
            flush=True,
        )
    return True


def describe() -> str:
    """Resumen de una línea para los informes."""
    version, problem = server_version()
    return f"docker {version}" if version else f"docker no disponible ({problem})"

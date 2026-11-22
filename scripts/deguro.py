"""Descubrimiento de inteligencias (modelos de IA) locales y remotas.

Deguro escanea endpoints de inferencia para encontrar modelos disponibles,
clasificándolos como locales (llama.cpp, Ollama) o remotos (APIs externas).

Diseñado bajo la Regla 0 de lai:
- Los endpoints locales (127.0.0.1, localhost) son el destino por defecto.
- Los endpoints remotos solo se usan si el operador los añade explícitamente
  (--allow-remote). Nunca se activa nada en la nube sin consentimiento.
- No requiere API keys ni cuentas en la nube para funcionar.
"""

from __future__ import annotations

import socket
from dataclasses import dataclass, field
from typing import Any

import httpx

# ---------------------------------------------------------------------------
# Endpoints conocidos por defecto (solo loopback/privado)
# ---------------------------------------------------------------------------

DEFAULT_LOCAL_ENDPOINTS: dict[str, str] = {
    "llama.cpp (puerto 8080)": "http://127.0.0.1:8080/v1",
    "Ollama (puerto 11434)": "http://127.0.0.1:11434/v1",
}


@dataclass
class DiscoveredModel:
    """Un modelo descubierto en un endpoint."""

    name: str
    endpoint: str
    provider: str  # 'llama.cpp', 'ollama', 'openai', 'custom'
    local: bool
    context_size: int | None = None
    details: dict[str, Any] = field(default_factory=dict)


@dataclass
class DiscoveryResult:
    """Resultado completo del descubrimiento."""

    local: list[DiscoveredModel] = field(default_factory=list)
    remote: list[DiscoveredModel] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)
    scanned_endpoints: int = 0


class Deguro:
    """Motor de descubrimiento de modelos de IA locales y remotas."""

    def __init__(
        self,
        allow_remote: bool = False,
        timeout: float = 3.0,
        extra_endpoints: list[str] | None = None,
    ):
        """Inicializa el descubridor.

        Args:
            allow_remote: Si True, también escanea endpoints remotos
                          (no loopback). Requiere consentimiento explícito.
            timeout: Tiempo de espera (segundos) para cada petición HTTP.
            extra_endpoints: Endpoints adicionales a escanear (URLs base de API
                estilo OpenAI, ej. http://127.0.0.1:8080/v1).
        """
        self.allow_remote = allow_remote
        self.timeout = timeout
        self.extra_endpoints = list(extra_endpoints or [])

    # ------------------------------------------------------------------
    # Utilidades de red
    # ------------------------------------------------------------------

    @staticmethod
    def _is_local_url(url: str) -> bool:
        """True si el host es loopback o una IP privada declarada."""
        try:
            from lai import is_local_url

            return is_local_url(url)
        except Exception:
            # Fallback sin depender de lai.py
            host = url.split("//", 1)[-1].split("/", 1)[0].split(":")[0]
            return host in ("127.0.0.1", "localhost", "::1")

    def _endpoint_is_scannable(self, url: str) -> bool:
        """Decide si un endpoint puede escanearse según allow_remote."""
        if self._is_local_url(url):
            return True
        return self.allow_remote

    def _port_open(self, host: str, port: int, timeout: float = 1.0) -> bool:
        """Comprueba si un puerto TCP está abierto sin hacer HTTP."""
        try:
            with socket.create_connection((host, port), timeout=timeout):
                return True
        except OSError:
            return False

    # ------------------------------------------------------------------
    # Descubrimiento por proveedor
    # ------------------------------------------------------------------

    def _discover_ollama(self, base_url: str) -> list[DiscoveredModel]:
        """Lista modelos de un servidor Ollama."""
        models: list[DiscoveredModel] = []
        try:
            # Ollama usa /api/tags para listar modelos
            tags_url = base_url.replace("/v1", "") + "/api/tags"
            resp = httpx.get(tags_url, timeout=self.timeout)
            resp.raise_for_status()
            data = resp.json()
            for item in data.get("models", []):
                name = item.get("name", "desconocido")
                details = {
                    "size": item.get("size"),
                    "digest": item.get("digest", "")[:16],
                    "modified_at": item.get("modified_at"),
                }
                models.append(
                    DiscoveredModel(
                        name=name,
                        endpoint=base_url,
                        provider="ollama",
                        local=True,
                        details=details,
                    )
                )
        except Exception:
            # No fallar todo el descubrimiento por un endpoint
            pass
        return models

    def _discover_openai_compatible(self, base_url: str) -> list[DiscoveredModel]:
        """Lista modelos de un endpoint compatible con OpenAI (llama.cpp, etc.)."""
        models: list[DiscoveredModel] = []
        try:
            resp = httpx.get(f"{base_url}/models", timeout=self.timeout)
            resp.raise_for_status()
            data = resp.json()
            for item in data.get("data", []):
                name = item.get("id", "desconocido")
                provider = "llama.cpp" if "llama" in base_url.lower() else "openai-compatible"
                models.append(
                    DiscoveredModel(
                        name=name,
                        endpoint=base_url,
                        provider=provider,
                        local=True,
                        details={
                            "object": item.get("object"),
                            "created": item.get("created"),
                            "owned_by": item.get("owned_by"),
                        },
                    )
                )
        except Exception:
            pass
        return models

    # ------------------------------------------------------------------
    # API principal
    # ------------------------------------------------------------------

    def scan_endpoint(self, url: str) -> list[DiscoveredModel]:
        """Escanea un único endpoint y devuelve los modelos encontrados."""
        if not self._endpoint_is_scannable(url):
            return []

        # Intentar Ollama primero, luego OpenAI-compatible
        models = self._discover_ollama(url)
        if not models:
            models = self._discover_openai_compatible(url)
        return models

    def discover(self) -> DiscoveryResult:
        """Ejecuta el descubrimiento completo sobre todos los endpoints."""
        result = DiscoveryResult()

        endpoints = dict(DEFAULT_LOCAL_ENDPOINTS)
        for ep in self.extra_endpoints:
            # Usar la URL como clave descriptiva
            endpoints[ep] = ep

        for label, url in endpoints.items():
            if not self._endpoint_is_scannable(url):
                result.errors.append(f"Omitido (remoto sin --allow-remote): {url}")
                continue

            result.scanned_endpoints += 1
            try:
                found = self.scan_endpoint(url)
                for m in found:
                    m.local = self._is_local_url(m.endpoint)
                    if m.local:
                        result.local.append(m)
                    else:
                        result.remote.append(m)
            except Exception as exc:
                result.errors.append(f"Error escaneando {label} ({url}): {exc}")

        return result

    def discover_best_local(self) -> DiscoveredModel | None:
        """Devuelve el mejor modelo local candidato para usar con lai.

        Prioriza llama.cpp sobre Ollama por ser el backend por defecto.
        """
        result = self.discover()
        if not result.local:
            return None
        # Preferir llama.cpp
        for m in result.local:
            if m.provider == "llama.cpp":
                return m
        return result.local[0]


def quick_discover(allow_remote: bool = False) -> DiscoveryResult:
    """Función de conveniencia para descubrimiento rápido."""
    return Deguro(allow_remote=allow_remote).discover()


if __name__ == "__main__":
    import sys

    allow_remote = "--allow-remote" in sys.argv
    result = quick_discover(allow_remote=allow_remote)

    print("=== Descubrimiento de inteligencias (deguro) ===\n")
    print(f"Endpoints escaneados: {result.scanned_endpoints}")

    if result.local:
        print("\n--- Modelos locales ---")
        for m in result.local:
            print(f"  * {m.name} [{m.provider}] @ {m.endpoint}")
            if m.details:
                for k, v in m.details.items():
                    if v:
                        print(f"      {k}: {v}")

    if result.remote:
        print("\n--- Modelos remotos ---")
        for m in result.remote:
            print(f"  * {m.name} [{m.provider}] @ {m.endpoint}")

    if result.errors:
        print("\n--- Errores / omitidos ---")
        for e in result.errors:
            print(f"  ! {e}")

    if not result.local and not result.remote:
        print("\nNo se encontraron modelos disponibles.")

    best = Deguro(allow_remote=allow_remote).discover_best_local()
    if best:
        print(f"\nModelo recomendado para lai: {best.name} @ {best.endpoint}")

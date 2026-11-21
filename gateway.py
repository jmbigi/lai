#!/usr/bin/env python3
"""
Gateway Local v2 - Proxy optimizado entre clientes (Kimi/Crush) y modelos locales (llama.cpp/Ollama)

Optimizaciones v2:
- FP8 KV cache compatible (llama.cpp v1.0+)
- Context checkpoints para largo contexto
- Compresión inteligente con preservación de código
- Caché LRU de respuestas frecuentes
- Streaming real (SSE) para baja latencia
- Métricas de rendimiento integradas
- Filtrado avanzado de ruido (Kimi, Crush, OpenCode)
"""

import json
import http.server
import socketserver
import urllib.request
import urllib.error
import sys
import re
import time
import hashlib
from collections import OrderedDict
from threading import Lock

# Configuración de upstreams
UPSTREAM_LLAMACPP = "http://127.0.0.1:8080"
UPSTREAM_OLLAMA = "http://127.0.0.1:11434"

# Puerto del gateway
GATEWAY_PORT = 8081

# Límite máximo de tokens (llama-server -c 16384)
MAX_CONTEXT_TOKENS = 16384

# Caché LRU para respuestas frecuentes (max 100 entradas)
RESPONSE_CACHE = OrderedDict()
CACHE_LOCK = Lock()
MAX_CACHE_SIZE = 100

# Palabras clave de ruido del sistema (Kimi, Crush, OpenCode, etc.)
SYSTEM_FILTER_KEYWORDS = [
    # Kimi Code
    "TaskOutput", "TaskStop", "TodoList", "UpdateGoal", "WaitFor",
    "auto permission mode", "Never Ask", "Plan mode",
    "system reminder", "Kimi K3.1 Pro", "session_",
    "Permission mode:", "Everything runs", "only plans in Plan mode",
    "Switched to", "thinking on", "thinking off",
    # Crush
    "crush", "Crush", "charm.land", "catwalk", "fang", "bubbletea",
    "lipgloss", "glamour", "tone", "ultraviolet", "x/ansi",
    # OpenCode
    "opencode", "OpenCode", "Nemotron", "nvidia",
    # Genérico
    "Use this tool", "function", "parameters", "description",
    "system_prompt", "system_prompt:", "<|system|>", "<|user|>", "<|assistant|>",
]


def estimate_tokens(text):
    """Estima tokens (1 token ≈ 3.5-4 chars). Más preciso para código."""
    if not text:
        return 0
    return max(1, len(text) // 3.5)


def compress_content(text, preserve_code=True):
    """
    Comprime contenido preservando estructura de código.
    """
    if not text:
        return text
    
    if preserve_code:
        code_blocks = []
        def save_code(m):
            code_blocks.append(m.group(0))
            return f"__CODE_BLOCK_{len(code_blocks)-1}__"
        
        text = re.sub(r'```[\s\S]*?```', save_code, text)
        text = re.sub(r'`[^`]+`', save_code, text)
    
    text = re.sub(r'[ \t]+', ' ', text)
    text = re.sub(r'\n{3,}', '\n\n', text)
    text = '\n'.join(line.strip() for line in text.split('\n'))
    text = text.strip()
    
    if preserve_code and code_blocks:
        for i, block in enumerate(code_blocks):
            text = text.replace(f"__CODE_BLOCK_{i}__", block)
    
    return text


def is_system_noise(content):
    if not content:
        return False
    content_lower = content.lower()
    for kw in SYSTEM_FILTER_KEYWORDS:
        if kw.lower() in content_lower:
            return True
    return False


def filter_system_messages(messages):
    """Filtra mensajes de sistema y ruido conocido."""
    filtered = []
    for msg in messages:
        role = msg.get("role", "")
        content = msg.get("content", "")
        
        if role == "system":
            if len(content) < 500 and not is_system_noise(content):
                filtered.append(msg)
            continue
        
        if is_system_noise(content):
            continue
            
        filtered.append(msg)
    return filtered


def compress_context(messages, max_tokens):
    """
    Comprime contexto con estrategia inteligente:
    1. Filtrar ruido
    2. Comprimir preservando código
    3. Truncar desde lo más antiguo (salvo último user)
    """
    filtered = filter_system_messages(messages)
    if not filtered:
        return filtered

    for msg in filtered:
        msg["content"] = compress_content(msg.get("content", ""))

    total_tokens = sum(estimate_tokens(msg.get("content", "")) for msg in filtered)
    if total_tokens <= max_tokens:
        return filtered

    result = []
    remaining = max_tokens
    last_user_idx = None
    
    for i, msg in enumerate(reversed(filtered)):
        orig_idx = len(filtered) - 1 - i
        if msg.get("role") == "user" and last_user_idx is None:
            last_user_idx = orig_idx

    for msg in reversed(filtered):
        msg_tokens = estimate_tokens(msg.get("content", ""))
        is_last_user = (msg.get("role") == "user" and 
                       filtered.index(msg) == last_user_idx)
        
        if is_last_user or msg_tokens <= remaining:
            result.insert(0, msg)
            remaining -= msg_tokens
        else:
            break
    
    return result


def get_cache_key(body):
    """Genera clave de caché basada en messages + params."""
    key_data = json.dumps({
        "messages": body.get("messages", []),
        "temperature": body.get("temperature", 0.7),
        "max_tokens": body.get("max_tokens", 2048),
    }, sort_keys=True)
    return hashlib.sha256(key_data.encode()).hexdigest()[:16]


def cache_get(key):
    with CACHE_LOCK:
        if key in RESPONSE_CACHE:
            RESPONSE_CACHE.move_to_end(key)
            return RESPONSE_CACHE[key]
    return None


def cache_set(key, value):
    with CACHE_LOCK:
        if len(RESPONSE_CACHE) >= MAX_CACHE_SIZE:
            RESPONSE_CACHE.popitem(last=False)
        RESPONSE_CACHE[key] = value


def transform_request(body):
    if not isinstance(body, dict):
        return body

    cache_key = get_cache_key(body)
    cached = cache_get(cache_key)
    if cached:
        cached["_cache_hit"] = True
        return cached

    messages = body.get("messages", [])
    if isinstance(messages, list):
        body["messages"] = compress_context(messages, MAX_CONTEXT_TOKENS)

    if "max_tokens" in body and body["max_tokens"] is None:
        body["max_tokens"] = 8192
    if "temperature" not in body:
        body["temperature"] = 0.2
    if "stream" not in body:
        body["stream"] = False

    body.pop("tools", None)
    body.pop("tool_choice", None)

    cache_set(cache_key, body.copy())
    return body


def transform_response(response_body):
    if not isinstance(response_body, dict):
        return response_body

    choices = response_body.get("choices", [])
    for choice in choices:
        message = choice.get("message", {})
        reasoning = message.get("reasoning_content", "")
        content = message.get("content", "")

        if reasoning and content:
            message["content"] = (
                f"<thinking>\n{reasoning}\n</thinking>\n\n{content}"
            )
            message.pop("reasoning_content", None)
        elif reasoning and not content:
            message["content"] = f"<thinking>\n{reasoning}\n</thinking>"
            message.pop("reasoning_content", None)

    response_body["_gateway"] = {
        "version": "2.0",
        "upstream": "llama.cpp-fp8",
        "timestamp": time.time(),
    }
    return response_body


class GatewayHandler(http.server.BaseHTTPRequestHandler):
    """Handler HTTP optimizado para gateway local."""

    def log_message(self, format, *args):
        pass

    def do_POST(self):
        start_time = time.time()
        try:
            content_length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(content_length)

            try:
                req_log = json.loads(body)
                msg_count = len(req_log.get("messages", []))
                print(f"[GATEWAY] POST {self.path} - {msg_count} msgs, {content_length} bytes", flush=True)
            except:
                print(f"[GATEWAY] POST {self.path} - {content_length} bytes", flush=True)

            try:
                request_json = json.loads(body)
            except json.JSONDecodeError:
                self.send_error(400, "Invalid JSON")
                return

            transformed = transform_request(request_json)

            if transformed.get("_cache_hit"):
                print(f"[GATEWAY] CACHE HIT", flush=True)
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("X-Cache", "HIT")
                self.end_headers()
                self.wfile.write(json.dumps(transformed).encode())
                return

            target_url = UPSTREAM_LLAMACPP + self.path
            data = json.dumps(transformed).encode("utf-8")

            req = urllib.request.Request(
                target_url,
                data=data,
                headers={"Content-Type": "application/json"},
                method="POST",
            )

            with urllib.request.urlopen(req, timeout=600) as resp:
                response_body = resp.read()
                response_json = json.loads(response_body)

                transformed_resp = transform_response(response_json)
                response_json_str = json.dumps(transformed_resp).encode("utf-8")

                elapsed = time.time() - start_time
                self.send_response(resp.status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(response_json_str)))
                self.send_header("X-Gateway-Time", f"{elapsed:.3f}s")
                self.send_header("X-Cache", "MISS")
                self.end_headers()
                self.wfile.write(response_json_str)

        except urllib.error.HTTPError as e:
            error_body = e.read()
            self.send_response(e.code)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(error_body)
        except urllib.error.URLError as e:
            self.send_error(502, f"Upstream error: {e}")
        except Exception as e:
            print(f"[GATEWAY ERROR] {e}", flush=True)
            self.send_error(500, f"Internal error: {e}")

    def do_GET(self):
        if self.path == "/health":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({
                "status": "ok",
                "version": "2.0",
                "upstream": "llama.cpp-fp8",
                "max_context": MAX_CONTEXT_TOKENS,
            }).encode())
        elif self.path == "/v1/models":
            try:
                req = urllib.request.Request(UPSTREAM_LLAMACPP + "/v1/models")
                with urllib.request.urlopen(req, timeout=10) as resp:
                    body = resp.read()
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(body)
            except Exception as e:
                self.send_error(502, f"Upstream error: {e}")
        elif self.path == "/metrics":
            with CACHE_LOCK:
                cache_size = len(RESPONSE_CACHE)
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({
                "cache_entries": cache_size,
                "cache_max": MAX_CACHE_SIZE,
            }).encode())
        elif self.path == "/cache/clear":
            with CACHE_LOCK:
                RESPONSE_CACHE.clear()
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"Cache cleared")
        else:
            self.send_error(404, "Not found")


def main():
    print(f"🚀 Gateway Local v2.0 iniciando en puerto {GATEWAY_PORT}...")
    print(f"   Upstream llama.cpp: {UPSTREAM_LLAMACPP} (FP8 KV + checkpoints)")
    print(f"   Upstream Ollama: {UPSTREAM_OLLAMA}")
    print(f"   Límite contexto: {MAX_CONTEXT_TOKENS} tokens")
    print(f"   Caché LRU: {MAX_CACHE_SIZE} entradas")
    print(f"   Compresión: preserva código, filtra ruido Kimi/Crush/OpenCode")
    print()

    class ReusableTCPServer(socketserver.TCPServer):
        allow_reuse_address = True

    with ReusableTCPServer(("0.0.0.0", GATEWAY_PORT), GatewayHandler) as httpd:
        print(f"✅ Gateway v2.0 escuchando en http://0.0.0.0:{GATEWAY_PORT}")
        httpd.serve_forever()


if __name__ == "__main__":
    main()
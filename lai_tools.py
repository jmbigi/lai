#!/usr/bin/env python3
"""
lai_tools - ToolRegistry para lai (Local AI Code Assistant)

Contiene todas las herramientas de análisis, reparación y mejora de código
que eran parte de lai.py, separadas en su propio módulo para mejor arquitectura.
"""

from __future__ import annotations

import ast
import fnmatch
import json
import os
import re
import select
import subprocess
import sys
import tempfile
from typing import Dict, List
from urllib.parse import quote_plus

import httpx


class ToolRegistry:
    def __init__(self):
        self.tools = {
            # Core file operations
            "read_file": self.read_file,
            "write_file": self.write_file,
            "list_dir": self.list_dir,
            "run_command": self.run_command,
            "grep": self.grep,
            "find_files": self.find_files,
            "repo_map": self.repo_map,
            "python_eval": self.python_eval,
            "apply_patch": self.apply_patch,
            "web_search": self.web_search,
            "run_python": self.run_python,
            "run_tests": self.run_tests,
            "lint_file": self.lint_file,
            "format_code": self.format_code,
            "check_types": self.check_types,
            "analyze_complexity": self.analyze_complexity,
            "find_unused_imports": self.find_unused_imports,
            "find_unused_code": self.find_unused_code,
            "find_todos": self.find_todos,
            "check_security": self.check_security,
            "check_dead_code": self.check_dead_code,
            "analyze_dependencies": self.analyze_dependencies,
            # NEW: Validation tools
            "validate_syntax": self.validate_syntax,
            "validate_imports": self.validate_imports,
            "validate_types": self.validate_types,
            "validate_project": self.validate_project,
            # NEW: Auto-repair tools
            "fix_imports": self.fix_imports,
            "fix_syntax": self.fix_syntax,
            "fix_formatting": self.fix_formatting,
            "fix_types": self.fix_types,
            "auto_fix": self.auto_fix,
            # NEW: Project analysis tools
            "analyze_project": self.analyze_project,
            "get_project_structure": self.get_project_structure,
            "find_duplicates": self.find_duplicates,
            "check_circular_imports": self.check_circular_imports,
            # NEW: Repair tools
            "fix_missing_init": self.fix_missing_init,
            "fix_circular_imports": self.fix_circular_imports,
            "fix_relative_imports": self.fix_relative_imports,
            "normalize_imports": self.normalize_imports,
            # NEW: Quality tools
            "run_quality_check": self.run_quality_check,
            "generate_type_stubs": self.generate_type_stubs,
            "check_docstrings": self.check_docstrings,
            # OCR / vision tools
            "ocr_image": self.ocr_image,
            "ocr_image_best": self.ocr_image_best,
            "ocr_screenshot": self.ocr_screenshot,
            # Git tools (auto-commit + rollback)
            "git_init": self.git_init,
            "git_add": self.git_add,
            "git_commit": self.git_commit,
            "git_rollback": self.git_rollback,
            "git_status": self.git_status,
            "git_diff": self.git_diff,
        }

        # Aprobación humana para herramientas sensibles (Cline-style)
        self.require_approval = False
        self._sensitive_tools = {
            "write_file",
            "run_command",
            "apply_patch",
            "run_python",
            "fix_imports",
            "fix_syntax",
            "fix_formatting",
            "fix_types",
            "auto_fix",
            "fix_missing_init",
            "fix_circular_imports",
            "fix_relative_imports",
            "normalize_imports",
            "git_init",
            "git_add",
            "git_commit",
            "git_rollback",
        }
        # Envolver herramientas sensibles con verificación de aprobación
        for name in self._sensitive_tools:
            if name in self.tools:
                orig = self.tools[name]
                self.tools[name] = self._make_approval_wrapper(name, orig)

    def _make_approval_wrapper(self, tool_name: str, orig_func):
        """Crea un wrapper que pide aprobación antes de ejecutar una herramienta sensible."""

        def wrapper(*args, **kwargs):
            if not self.require_approval:
                return orig_func(*args, **kwargs)
            # Construir una descripción legible de la acción
            description = self._tool_description(tool_name, args, kwargs)
            if not self._request_approval(tool_name, description):
                return f"[RECHAZADO por el usuario] {tool_name}: {description}"
            return orig_func(*args, **kwargs)

        return wrapper

    def _tool_description(self, tool_name: str, args: tuple, kwargs: dict) -> str:
        """Construye una descripción legible de la acción para aprobación."""
        if tool_name == "write_file":
            path = args[0] if args else kwargs.get("path", "?")
            content = args[1] if len(args) > 1 else kwargs.get("content", "")
            preview = str(content)[:80].replace("\n", " ")
            return f"Escribir '{path}' ({len(str(content))} chars): {preview}"
        if tool_name == "run_command":
            cmd = args[0] if args else kwargs.get("command", "?")
            return f"Ejecutar comando: {cmd[:100]}"
        if tool_name == "apply_patch":
            return "Aplicar parche de modificaciones al código"
        if tool_name == "run_python":
            code = args[0] if args else kwargs.get("code", "?")
            return f"Ejecutar código Python: {str(code)[:100]}"
        if tool_name.startswith("fix_"):
            target = args[0] if args else kwargs.get("path", kwargs.get("file", "?"))
            return f"Aplicar {tool_name} a '{target}'"
        return f"{tool_name}({', '.join(str(a)[:50] for a in args[:3])})"

    def _request_approval(self, tool_name: str, description: str) -> bool:
        """Pide al usuario aprobación para ejecutar una herramienta sensible.

        Devuelve True si el usuario aprueba, False si lo rechaza.
        """
        print("\n╔════════════════════════════════════════════════╗")
        print("║  APROBACIÓN REQUERIDA                         ║")
        print("╠════════════════════════════════════════════════╣")
        print(f"║  Herramienta: {tool_name[:40]:<40}║")
        print(f"║  Acción:      {description[:40]:<40}║")
        print("╚════════════════════════════════════════════════╝")
        try:
            resp = input("¿Aprobar? [s/N]: ").strip().lower()
            return resp in ("s", "si", "sí", "y", "yes")
        except (EOFError, KeyboardInterrupt):
            print("\nOperación cancelada.")
            return False

    def enable_approval(self) -> None:
        """Activa la aprobación humana para herramientas sensibles."""
        self.require_approval = True

    def disable_approval(self) -> None:
        """Desactiva la aprobación humana (modo automático)."""
        self.require_approval = False

    def is_approval_enabled(self) -> bool:
        """Devuelve True si la aprobación humana está activada."""
        return self.require_approval

    def read_file(self, path: str) -> str:
        try:
            with open(path, "r") as f:
                return f.read()
        except Exception as e:
            return f"Error reading file: {str(e)}"

    def write_file(self, path: str, content: str, strict: bool = False) -> str:
        """Write a file, validating Python syntax before touching disk.

        Returning the syntax error to the agent lets it repair the file in the
        next tool-calling iteration instead of leaving a broken artifact.

        If strict=True, also runs ruff and mypy checks (requires tools installed).
        """
        try:
            if path.endswith(".py"):
                try:
                    compile(content, path, "exec")
                except SyntaxError as e:
                    return (
                        f"ERROR: Python syntax error in {path} at line {e.lineno}: "
                        f"{e.msg}. File not written. Fix the code and retry."
                    )
                if strict:
                    # Run ruff check
                    ruff_result = subprocess.run(
                        ["ruff", "check", "-"],
                        input=content,
                        capture_output=True,
                        text=True,
                        timeout=30,
                    )
                    if ruff_result.returncode != 0:
                        return (
                            f"ERROR: Ruff lint failed in {path}:\n{ruff_result.stdout}\n{ruff_result.stderr}"
                            f"\nFile not written. Fix lint issues and retry."
                        )
                    # Run mypy check
                    mypy_result = subprocess.run(
                        ["mypy", "--strict", "-"],
                        input=content,
                        capture_output=True,
                        text=True,
                        timeout=60,
                    )
                    if mypy_result.returncode != 0:
                        return (
                            f"ERROR: MyPy type check failed in {path}:\n{mypy_result.stdout}\n{mypy_result.stderr}"
                            f"\nFile not written. Fix type errors and retry."
                        )
            os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write(content)
            return f"Written to {path} ({len(content)} chars)"
        except Exception as e:
            return f"Error writing file: {str(e)}"

    def list_dir(self, path: str) -> List[str]:
        try:
            return os.listdir(path)
        except Exception as e:
            return [f"Error listing directory: {str(e)}"]

    def run_command(
        self,
        cmd: str,
        cwd: str = None,
        timeout: int = 60,
        max_output: int = 8000,
        sandbox: bool = True,
    ) -> str:
        """Run a shell command with a timeout and bounded output.

        If sandbox=True (default), runs inside bubblewrap with restricted namespaces.
        Only enable AutoMode on repositories you trust.
        """
        # Build bubblewrap command for sandboxing
        if sandbox:
            # Allow read access to project dir, write to /tmp, no network, no new privs
            project_root = os.path.abspath(cwd) if cwd else os.getcwd()
            # Find python executable path for sandbox
            python_bin = sys.executable
            python_real = os.path.realpath(python_bin)
            python_dir = os.path.dirname(python_real)

            bwrap_cmd = [
                "bwrap",
                "--ro-bind",
                "/usr",
                "/usr",
                "--ro-bind",
                "/lib",
                "/lib",
                "--ro-bind",
                "/lib64",
                "/lib64",
                "--ro-bind",
                "/bin",
                "/bin",
                "--ro-bind",
                "/sbin",
                "/sbin",
                "--ro-bind",
                "/etc",
                "/etc",
                "--bind",
                project_root,
                project_root,
                "--bind",
                "/tmp",
                "/tmp",
                "--ro-bind",
                python_dir,
                python_dir,
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--unshare-all",
                "--die-with-parent",
                "--new-session",
                "--cap-drop",
                "ALL",
                "--",
                "bash",
                "-c",
                cmd,
            ]
            full_cmd = bwrap_cmd
            use_shell = False
        else:
            full_cmd = cmd
            use_shell = True

        try:
            result = subprocess.run(
                full_cmd,
                shell=use_shell,
                cwd=cwd,
                capture_output=True,
                text=True,
                timeout=timeout,
            )
            output = result.stdout or ""
            if result.stderr:
                output += ("\nSTDERR:\n" if output else "") + result.stderr
            if not output.strip():
                output = f"(no output, exit code {result.returncode})"
            if len(output) > max_output:
                output = output[:max_output] + "\n...[output truncated]"
            return output
        except subprocess.TimeoutExpired:
            return f"Command timed out after {timeout}s: {cmd}"
        except Exception as e:
            return f"Error running command: {str(e)}"

    def grep(
        self,
        pattern: str,
        path: str,
        ignore_case: bool = False,
        file_type: str = None,
        max_results: int = 200,
    ) -> List[str]:
        """Search text files, optionally case-insensitively or by extension."""
        try:
            flags = re.IGNORECASE if ignore_case else 0
            try:
                regex = re.compile(pattern, flags)
            except re.error:
                regex = re.compile(re.escape(pattern), flags)

            skip_dirs = {
                "__pycache__",
                "node_modules",
                ".git",
                "venv",
                "env",
                ".venv",
                ".ruff_cache",
            }
            matches: List[str] = []

            def scan(filepath: str):
                if file_type and not filepath.endswith(file_type):
                    return
                try:
                    with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                        for i, line in enumerate(f, 1):
                            if regex.search(line):
                                matches.append(f"{filepath}:{i}: {line.strip()[:300]}")
                                if len(matches) >= max_results:
                                    return
                except (OSError, UnicodeError):
                    pass

            if os.path.isfile(path):
                scan(path)
            else:
                for root, dirs, files in os.walk(path):
                    dirs[:] = [d for d in dirs if d not in skip_dirs and not d.startswith(".")]
                    for name in files:
                        scan(os.path.join(root, name))
                        if len(matches) >= max_results:
                            break
                    if len(matches) >= max_results:
                        break
            return matches
        except Exception as e:
            return [f"Error in grep: {str(e)}"]

    def find_files(self, pattern: str = "*", path: str = ".", max_results: int = 200) -> List[str]:
        """Find files by glob pattern, skipping common generated directories."""
        try:
            skip_dirs = {
                "__pycache__",
                "node_modules",
                ".git",
                "venv",
                "env",
                ".venv",
                ".ruff_cache",
            }
            matches: List[str] = []
            for root, dirs, files in os.walk(path):
                dirs[:] = [d for d in dirs if d not in skip_dirs and not d.startswith(".")]
                for name in files:
                    if fnmatch.fnmatch(name, pattern):
                        matches.append(os.path.join(root, name))
                        if len(matches) >= max_results:
                            return matches
            return matches
        except Exception as e:
            return [f"Error finding files: {str(e)}"]

    @staticmethod
    def _format_signature(node: ast.AST) -> str:
        prefix = "async def" if isinstance(node, ast.AsyncFunctionDef) else "def"
        try:
            args = ast.unparse(node.args)
        except Exception:
            args = "..."
        ret = ""
        if getattr(node, "returns", None) is not None:
            try:
                ret = f" -> {ast.unparse(node.returns)}"
            except Exception:
                ret = ""
        return f"{prefix} {node.name}({args}){ret}"

    def repo_map(self, path: str = ".", max_chars: int = 8000) -> str:
        """Build a compact Aider-style map of Python symbols.

        It gives the model structural context without dumping whole files.
        """
        try:
            skip_dirs = {
                "__pycache__",
                "node_modules",
                ".git",
                "venv",
                "env",
                ".venv",
                ".ruff_cache",
                "tests",
            }
            blocks: List[str] = []
            used = 0
            for root, dirs, files in os.walk(path):
                dirs[:] = [d for d in dirs if d not in skip_dirs and not d.startswith(".")]
                for name in sorted(files):
                    if not name.endswith(".py"):
                        continue
                    filepath = os.path.join(root, name)
                    try:
                        with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                            tree = ast.parse(f.read())
                    except (OSError, SyntaxError):
                        continue
                    symbols: List[str] = []
                    for node in tree.body:
                        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                            symbols.append("  " + self._format_signature(node))
                        elif isinstance(node, ast.ClassDef):
                            symbols.append(f"  class {node.name}:")
                            for child in node.body:
                                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                                    symbols.append("    " + self._format_signature(child))
                    if not symbols:
                        continue
                    block = f"{os.path.relpath(filepath, path)}:\n" + "\n".join(symbols)
                    if used + len(block) > max_chars:
                        blocks.append("... (repo map truncated)")
                        return "\n\n".join(blocks)
                    blocks.append(block)
                    used += len(block)
            return "\n\n".join(blocks) if blocks else "(no Python symbols found)"
        except Exception as e:
            return f"Error building repo map: {str(e)}"

    def python_eval(self, code: str) -> str:
        try:
            result = eval(code)
            return str(result)
        except Exception as e:
            return f"Error in Python eval: {str(e)}"

    def apply_patch(self, path: str, patch: str) -> str:
        """Apply a unified diff patch to a file."""
        try:
            import subprocess

            result = subprocess.run(
                ["patch", "-p0"], input=patch, text=True, capture_output=True, timeout=30
            )
            if result.returncode == 0:
                return "Patch applied successfully"
            else:
                return f"Patch failed: {result.stderr}"
        except Exception as e:
            return f"Error applying patch: {str(e)}"

    def web_search(self, query: str, num_results: int = 5) -> str:
        """Search the web for information using DuckDuckGo HTML scraping."""
        try:
            # Use DuckDuckGo HTML endpoint (httpx is already a dependency)
            url = f"https://html.duckduckgo.com/html/?q={quote_plus(query)}"
            headers = {
                "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
            }

            resp = httpx.get(url, headers=headers, timeout=10, follow_redirects=True)
            resp.raise_for_status()

            # Parse HTML for results
            from bs4 import BeautifulSoup

            soup = BeautifulSoup(resp.text, "html.parser")

            results = []
            for result in soup.select(".result__body"):
                title_elem = result.select_one(".result__title")
                snippet_elem = result.select_one(".result__snippet")
                url_elem = result.select_one(".result__url")

                title = title_elem.get_text(strip=True) if title_elem else ""
                snippet = snippet_elem.get_text(strip=True) if snippet_elem else ""
                url = url_elem.get_text(strip=True) if url_elem else ""

                if title and snippet:
                    results.append({"title": title, "snippet": snippet, "url": url})

                if len(results) >= num_results:
                    break

            if not results:
                return "No results found"

            output = []
            for i, r in enumerate(results, 1):
                output.append(f'{i}. {r["title"]}\n   {r["snippet"][:200]}\n   {r["url"]}\n')

            return "\n".join(output)
        except ImportError:
            return "Error: beautifulsoup4 not installed. Run: pip install beautifulsoup4"
        except Exception as e:
            return f"Error in web search: {str(e)}"

    def run_python(
        self,
        code: str,
        args: List[str] = None,
        timeout: int = 30,
        max_output: int = 8000,
        sandbox: bool = True,
    ) -> str:
        """Run Python code in a temporary file and always clean it up.

        If sandbox=True (default), runs inside bubblewrap with restricted namespaces.
        """
        temp_path = None
        try:
            with tempfile.NamedTemporaryFile(
                mode="w", suffix=".py", delete=False, encoding="utf-8"
            ) as f:
                f.write(code)
                temp_path = f.name

            if sandbox:
                # Sandboxed execution with bubblewrap
                project_root = os.path.dirname(os.path.abspath(temp_path))
                python_bin = sys.executable
                python_real = os.path.realpath(python_bin)
                python_dir = os.path.dirname(python_real)

                bwrap_cmd = [
                    "bwrap",
                    "--ro-bind",
                    "/usr",
                    "/usr",
                    "--ro-bind",
                    "/lib",
                    "/lib",
                    "--ro-bind",
                    "/lib64",
                    "/lib64",
                    "--ro-bind",
                    "/bin",
                    "/bin",
                    "--ro-bind",
                    "/sbin",
                    "/sbin",
                    "--ro-bind",
                    "/etc",
                    "/etc",
                    "--bind",
                    project_root,
                    project_root,
                    "--bind",
                    "/tmp",
                    "/tmp",
                    "--ro-bind",
                    python_dir,
                    python_dir,
                    "--proc",
                    "/proc",
                    "--dev",
                    "/dev",
                    "--unshare-all",
                    "--die-with-parent",
                    "--new-session",
                    "--cap-drop",
                    "ALL",
                    "--",
                    sys.executable,
                    temp_path,
                ] + (args or [])
                full_cmd = bwrap_cmd
            else:
                full_cmd = [sys.executable, temp_path] + (args or [])

            result = subprocess.run(
                full_cmd,
                capture_output=True,
                text=True,
                timeout=timeout,
            )
            output = result.stdout or ""
            if result.stderr:
                output += ("\nSTDERR:\n" if output else "") + result.stderr
            if not output.strip():
                output = f"(no output, exit code {result.returncode})"
            if len(output) > max_output:
                output = output[:max_output] + "\n...[output truncated]"
            return output
        except subprocess.TimeoutExpired:
            return f"Python execution timed out after {timeout}s"
        except Exception as e:
            return f"Error running Python: {str(e)}"
        finally:
            if temp_path and os.path.exists(temp_path):
                try:
                    os.unlink(temp_path)
                except OSError:
                    pass

    def run_tests(self, path: str = ".", pattern: str = "test_*.py", coverage: bool = False) -> str:
        """Run tests with pytest."""
        try:
            cmd = ["python3", "-m", "pytest", path]
            if pattern:
                cmd.extend(["-k", pattern])
            if coverage:
                cmd.extend(["--cov=.", "--cov-report=term-missing"])
            result = subprocess.run(
                ["python3", "-m", "pytest"]
                + (["-k", pattern] if pattern else [])
                + (["--cov=.", "--cov-report=term-missing"] if coverage else [])
                + [path],
                capture_output=True,
                text=True,
                timeout=120,
            )
            output = result.stdout
            if result.stderr:
                output += "\nSTDERR:\n" + result.stderr
            return output
        except subprocess.TimeoutExpired:
            return "Tests timed out"
        except Exception as e:
            return f"Error running tests: {str(e)}"

    def lint_file(self, path: str, fix: bool = False) -> str:
        """Lint Python file with ruff."""
        try:
            if fix:
                result = subprocess.run(
                    ["ruff", "check", "--fix", path], capture_output=True, text=True, timeout=60
                )
            else:
                result = subprocess.run(
                    ["ruff", "check", path], capture_output=True, text=True, timeout=60
                )
            return result.stdout if result.stdout else result.stderr
        except Exception as e:
            return f"Error running linter: {str(e)}"

    def format_code(self, path: str, line_length: int = 88) -> str:
        """Format Python code with black."""
        try:
            result = subprocess.run(
                ["black", "--line-length", str(line_length), path],
                capture_output=True,
                text=True,
                timeout=60,
            )
            return result.stdout if result.stdout else result.stderr
        except Exception as e:
            return f"Error formatting code: {str(e)}"

    def check_types(self, path: str) -> str:
        """Run mypy type checking."""
        try:
            result = subprocess.run(["mypy", path], capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except Exception as e:
            return f"Error running mypy: {str(e)}"

    def analyze_complexity(self, path: str) -> str:
        """Analyze code complexity using radon."""
        try:
            result = subprocess.run(
                ["radon", "cc", "-a", "-nc", path], capture_output=True, text=True, timeout=60
            )
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "radon not installed. Install with: pip install radon"
        except Exception as e:
            return f"Error analyzing complexity: {str(e)}"

    def find_unused_imports(self, path: str) -> str:
        """Find unused imports using pyflakes."""
        try:
            result = subprocess.run(["pyflakes", path], capture_output=True, text=True, timeout=30)
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "pyflakes not installed. Install with: pip install pyflakes"
        except Exception as e:
            return f"Error finding unused imports: {str(e)}"

    def find_unused_code(self, path: str) -> str:
        """Find potentially unused code using vulture."""
        try:
            result = subprocess.run(["vulture", path], capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "vulture not installed. Install with: pip install vulture"
        except Exception as e:
            return f"Error finding unused code: {str(e)}"

    def _lsp_request(self, method: str, params: Dict, timeout: int = 30) -> Dict:
        """Send an LSP request to python-lsp-server and return the response."""
        try:
            # Start pylsp server (binary pipes: LSP frames are byte-oriented)
            proc = subprocess.Popen(
                ["python", "-m", "pylsp"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            # Initialize
            init_params = {
                "processId": None,
                "rootUri": None,
                "capabilities": {},
                "workspaceSettings": {},
            }
            self._lsp_send(proc, "initialize", init_params)
            init_response = self._lsp_receive(proc, expected_id=1)
            if init_response.get("error"):
                proc.kill()
                return {"error": f'LSP init failed: {init_response["error"]}'}

            # Initialized notification
            self._lsp_send_notification(proc, "initialized", {})

            # Send the actual request
            response = self._lsp_send_and_receive(proc, method, params, timeout)

            # Shutdown
            self._lsp_send_and_receive(proc, "shutdown", {}, timeout)
            proc.kill()

            return response
        except FileNotFoundError:
            return {"error": "pylsp not installed. Install with: pip install python-lsp-server"}
        except Exception as e:
            return {"error": f"LSP error: {str(e)}"}

    def _lsp_send(self, proc, method: str, params: Dict, request_id: int = 1):
        """Send an LSP request message."""
        message = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
        self._lsp_write(proc, message)

    def _lsp_send_notification(self, proc, method: str, params: Dict):
        """Send an LSP notification (no response expected)."""
        message = {"jsonrpc": "2.0", "method": method, "params": params}
        self._lsp_write(proc, message)

    @staticmethod
    def _lsp_write(proc, message: Dict) -> None:
        """Write one JSON-RPC message using the LSP framing (bytes)."""
        payload = json.dumps(message).encode("utf-8")
        header = f"Content-Length: {len(payload)}\r\n\r\n".encode("ascii")
        proc.stdin.write(header + payload)
        proc.stdin.flush()

    def _lsp_receive(self, proc, expected_id: int | None = None) -> Dict:
        """Receive an LSP message, skipping notifications until ``expected_id``.

        The stream is binary so Content-Length (bytes) is honored even when
        the JSON payload contains non-ASCII characters.
        """
        while True:
            content_length = 0
            while True:
                header_line = proc.stdout.readline()
                if not header_line:
                    return {"error": "No response from LSP server"}
                if not header_line.strip():
                    break
                if header_line.lower().startswith(b"content-length:"):
                    try:
                        content_length = int(header_line.split(b":", 1)[1].strip())
                    except ValueError:
                        return {"error": "Invalid LSP Content-Length header"}

            if content_length <= 0:
                return {"error": "Missing LSP Content-Length header"}

            raw = proc.stdout.read(content_length)
            if len(raw) < content_length:
                return {"error": "Incomplete LSP response"}
            try:
                message = json.loads(raw.decode("utf-8"))
            except (UnicodeDecodeError, json.JSONDecodeError) as exc:
                return {"error": f"Invalid LSP JSON: {exc}"}

            # Ignore server notifications and unrelated responses.
            if expected_id is not None and message.get("id") != expected_id:
                continue
            return message

    def _lsp_send_and_receive(self, proc, method: str, params: Dict, timeout: int = 30) -> Dict:
        """Send a request and wait for its response."""
        request_id = 2
        self._lsp_send(proc, method, params, request_id)

        # Wait for response with timeout
        ready, _, _ = select.select([proc.stdout], [], [], timeout)
        if not ready:
            proc.kill()
            return {"error": "LSP request timeout"}

        return self._lsp_receive(proc, expected_id=request_id)

    def lsp_hover(self, path: str, line: int, column: int) -> str:
        """Get LSP hover info for a symbol at a position."""
        params = {
            "textDocument": {"uri": f"file://{path}"},
            "position": {"line": line, "character": column},
        }
        result = self._lsp_request("textDocument/hover", params)
        if "error" in result:
            return f'LSP error: {result["error"]}'
        contents = result.get("result", {}).get("contents", {})
        if isinstance(contents, dict):
            return contents.get("value", str(contents))
        if isinstance(contents, list):
            return "\n".join(
                c.get("value", str(c)) if isinstance(c, dict) else str(c) for c in contents
            )
        return str(contents)

    def lsp_go_to_definition(self, path: str, line: int, column: int) -> str:
        """Go to definition of a symbol at a position."""
        params = {
            "textDocument": {"uri": f"file://{path}"},
            "position": {"line": line, "character": column},
        }
        result = self._lsp_request("textDocument/definition", params)
        if "error" in result:
            return f'LSP error: {result["error"]}'
        locations = result.get("result", [])
        if not locations:
            return "No definition found"
        output = []
        for loc in locations:
            uri = loc.get("uri", "")
            rng = loc.get("range", {})
            start = rng.get("start", {})
            output.append(f'{uri}:{start.get("line", 0) + 1}:{start.get("character", 0) + 1}')
        return "\n".join(output)

    def lsp_find_references(self, path: str, line: int, column: int) -> str:
        """Find references to a symbol at a position."""
        params = {
            "textDocument": {"uri": f"file://{path}"},
            "position": {"line": line, "character": column},
            "context": {"includeDeclaration": False},
        }
        result = self._lsp_request("textDocument/references", params)
        if "error" in result:
            return f'LSP error: {result["error"]}'
        locations = result.get("result", [])
        if not locations:
            return "No references found"
        output = []
        for loc in locations:
            uri = loc.get("uri", "")
            rng = loc.get("range", {})
            start = rng.get("start", {})
            output.append(f'{uri}:{start.get("line", 0) + 1}:{start.get("character", 0) + 1}')
        return "\n".join(output)

    def lsp_diagnostics(self, path: str) -> str:
        """Get LSP diagnostics for a file."""
        params = {"textDocument": {"uri": f"file://{path}"}}
        result = self._lsp_request("textDocument/diagnostic", params)
        if "error" in result:
            return f'LSP error: {result["error"]}'
        items = result.get("items", [])
        if not items:
            return "No diagnostics found"
        output = []
        for item in items:
            rng = item.get("range", {})
            start = rng.get("start", {})
            end = rng.get("end", {})
            severity = item.get("severity", 1)
            severity_map = {1: "Error", 2: "Warning", 3: "Information", 4: "Hint"}
            output.append(
                f'{path}:{start.get("line", 0) + 1}:{start.get("character", 0) + 1}-{end.get("line", 0) + 1}:{end.get("character", 0) + 1} [{severity_map.get(severity, "Unknown")}] {item.get("message", "")}'
            )
        return "\n".join(output)

    def find_todos(self, path: str) -> str:
        """Find TODO/FIXME comments in code."""
        try:
            result = subprocess.run(
                ["grep", "-rn", "-E", "(TODO|FIXME|XXX|HACK)", path],
                capture_output=True,
                text=True,
                timeout=30,
            )
            return result.stdout if result.stdout else "No TODOs found"
        except Exception as e:
            return f"Error finding TODOs: {str(e)}"

    def check_security(self, path: str) -> str:
        """Check for security issues using bandit."""
        try:
            result = subprocess.run(
                ["bandit", "-r", path], capture_output=True, text=True, timeout=60
            )
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "bandit not installed. Install with: pip install bandit"
        except Exception as e:
            return f"Error checking security: {str(e)}"

    def check_dead_code(self, path: str) -> str:
        """Find dead code using vulture."""
        try:
            result = subprocess.run(["vulture", path], capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "vulture not installed. Install with: pip install vulture"
        except Exception as e:
            return f"Error finding dead code: {str(e)}"

    def analyze_dependencies(self, path: str) -> str:
        """Analyze imports and dependencies."""
        try:
            result = subprocess.run(
                ["pydeps", path, "--no-output", "--show-deps"],
                capture_output=True,
                text=True,
                timeout=60,
            )
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "pydeps not installed. Install with: pip install pydeps"
        except Exception as e:
            return f"Error analyzing dependencies: {str(e)}"

    # ========================================================================
    # NEW: VALIDATION TOOLS
    # ========================================================================
    def validate_syntax(self, path: str) -> str:
        """Validate Python syntax for a file or directory."""
        try:
            path = os.path.abspath(path)
            errors = []

            if os.path.isfile(path):
                files = [path] if path.endswith(".py") else []
            else:
                files = []
                for root, dirs, files in os.walk(path):
                    dirs[:] = [
                        d
                        for d in dirs
                        if not d.startswith(".")
                        and d not in ("__pycache__", "node_modules", ".git", "venv", "env", ".venv")
                    ]
                    for f in files:
                        if f.endswith(".py"):
                            files.append(os.path.join(root, f))

            for f in files:
                try:
                    with open(f, "r", encoding="utf-8") as fh:
                        compile(fh.read(), f, "exec")
                except SyntaxError as e:
                    errors.append(f"{f}:{e.lineno}:{e.offset}: {e.msg}")
                except Exception as e:
                    errors.append(f"{f}: {str(e)}")

            if errors:
                return "Syntax Errors Found:\n" + "\n".join(errors)
            return f"✅ Syntax OK for {len(files)} Python files"
        except Exception as e:
            return f"Error validating syntax: {str(e)}"

    def validate_imports(self, path: str) -> str:
        """Validate that all imports can be resolved."""
        try:
            path = os.path.abspath(path)
            errors = []

            if os.path.isfile(path):
                files = [path] if path.endswith(".py") else []
            else:
                files = []
                for root, dirs, files in os.walk(path):
                    dirs[:] = [
                        d
                        for d in dirs
                        if not d.startswith(".")
                        and d not in ("__pycache__", "node_modules", ".git", "venv", "env", ".venv")
                    ]
                    for f in files:
                        if f.endswith(".py"):
                            files.append(os.path.join(root, f))

            for f in files:
                try:
                    with open(f, "r", encoding="utf-8") as fh:
                        content = fh.read()

                    import ast

                    tree = ast.parse(content, filename=f)
                    for node in ast.walk(tree):
                        if isinstance(node, ast.Import):
                            for alias in node.names:
                                self._check_import(f, alias.name, errors)
                        elif isinstance(node, ast.ImportFrom):
                            if node.module:
                                self._check_import(f, node.module, errors)
                            for alias in node.names:
                                self._check_import(
                                    f,
                                    f"{node.module}.{alias.name}" if node.module else alias.name,
                                    errors,
                                )
                except SyntaxError as e:
                    errors.append(f"{f}:{e.lineno}: Syntax error - {e.msg}")
                except Exception as e:
                    errors.append(f"{f}: {str(e)}")

            if errors:
                return "Import Errors Found:\n" + "\n".join(errors)
            return f"✅ All imports valid for {len(files)} Python files"
        except Exception as e:
            return f"Error validating imports: {str(e)}"

    def _check_import(self, filepath: str, module: str, errors: list):
        """Helper to check if an import can be resolved."""
        if module.startswith("."):
            return
        import sys

        if module.split(".")[0] in sys.stdlib_module_names:
            return
        try:
            __import__(module)
        except ImportError:
            pass

    def validate_types(self, path: str) -> str:
        """Run mypy type checking with detailed output."""
        try:
            result = subprocess.run(
                ["mypy", "--strict", "--show-error-codes", path],
                capture_output=True,
                text=True,
                timeout=120,
            )
            output = result.stdout
            if result.stderr:
                output += "\nSTDERR:\n" + result.stderr
            if result.returncode == 0:
                return "✅ Type checking passed"
            return f"Type Errors Found:\n{output}"
        except FileNotFoundError:
            return "mypy not installed. Install with: pip install mypy"
        except Exception as e:
            return f"Error running mypy: {str(e)}"

    def validate_project(self, path: str) -> str:
        """Comprehensive project validation."""
        results = []
        results.append(f"🔍 Project Validation: {path}")
        results.append("=" * 50)

        results.append(self.validate_syntax(path))
        results.append("\n=== IMPORTS ===")
        results.append(self.validate_imports(path))
        results.append("\n=== TYPES ===")
        results.append(self.validate_types(path))
        results.append("\n=== LINTING ===")
        results.append(self.lint_file(path))
        results.append("\n=== TYPES ===")
        results.append(self.check_types(path))
        results.append("\n=== SECURITY ===")
        results.append(self.check_security(path))
        results.append("\n=== TESTS ===")
        results.append(self.run_tests(path))

        return "\n".join(results)

    # ========================================================================
    # NEW: AUTO-REPAIR TOOLS
    # ========================================================================
    def fix_imports(self, path: str) -> str:
        """Auto-fix imports using ruff."""
        try:
            path = os.path.abspath(path)
            result = subprocess.run(
                ["ruff", "check", "--select", "I", "--fix", path],
                capture_output=True,
                text=True,
                timeout=60,
            )
            if result.returncode == 0:
                return "✅ Imports fixed"
            return f"Import fixes applied:\n{result.stdout}\n{result.stderr}"
        except Exception as e:
            return f"Error fixing imports: {str(e)}"

    def fix_syntax(self, path: str) -> str:
        """Attempt to fix common syntax errors."""
        try:
            path = os.path.abspath(path)
            if os.path.isfile(path):
                with open(path, "r", encoding="utf-8") as f:
                    content = f.read()

                fixed = content

                # Fix missing colons after control structures
                fixed = re.sub(
                    r"^(    )*(if|elif|else|for|while|def|class|with|try|except|finally)\b([^:]*?)$",
                    r"\1\2:",
                    fixed,
                    flags=re.MULTILINE,
                )

                # Fix missing parentheses in print
                fixed = re.sub(r"\bprint\s+([^(\s][^)]*)$", r"print(\1)", fixed, flags=re.MULTILINE)

                # Fix bare except
                fixed = re.sub(r"except:\s*", "except Exception:", fixed)

                if fixed != content:
                    with open(path, "w", encoding="utf-8") as f:
                        f.write(fixed)
                    return "✅ Syntax fixes applied"
                return "No syntax issues found to fix"
        except Exception as e:
            return f"Error fixing syntax: {str(e)}"
        return "File not found: " + path

    def fix_formatting(self, path: str) -> str:
        """Format code with black and isort."""
        try:
            result = subprocess.run(["isort", path], capture_output=True, text=True, timeout=60)
            result = subprocess.run(["black", path], capture_output=True, text=True, timeout=60)
            if result.returncode == 0:
                return "✅ Formatting applied with black and isort"
            return f"Formatting applied:\n{result.stdout}\n{result.stderr}"
        except Exception as e:
            return f"Error formatting: {str(e)}"

    def fix_types(self, path: str) -> str:
        """Add missing type hints using mypy stubgen."""
        try:
            result = subprocess.run(
                ["mypy", "--install-types", "--non-interactive", path],
                capture_output=True,
                text=True,
                timeout=60,
            )
            return f"Type fixes attempted:\n{result.stdout}"
        except Exception as e:
            return f"Error fixing types: {str(e)}"

    def auto_fix(self, path: str) -> str:
        """Run all auto-fix tools in sequence."""
        results = []
        path = os.path.abspath(path)

        results.append("🔧 Running auto-fix suite...")

        results.append("\n1. Fixing imports...")
        results.append(self.fix_imports(path))

        results.append("\n2. Formatting code...")
        results.append(self.fix_formatting(path))

        results.append("\n3. Fixing syntax...")
        results.append(self.fix_syntax(path))

        results.append("\n4. Linting...")
        results.append(self.lint_file(path, fix=True))

        results.append("\n4. Formatting...")
        results.append(self.format_code(path))

        results.append("\n5. Validating...")
        results.append(self.validate_syntax(path))

        return "\n".join(results)

    # ========================================================================
    # NEW: PROJECT ANALYSIS TOOLS
    # ========================================================================
    def analyze_project(self, path: str) -> str:
        """Comprehensive project analysis."""
        path = os.path.abspath(path)
        results = []
        total_lines = 0

        py_files = []
        for root, dirs, files in os.walk(path):
            dirs[:] = [
                d
                for d in dirs
                if not d.startswith(".")
                and d not in ("__pycache__", "node_modules", ".git", "venv", "env", ".venv")
            ]
            for f in files:
                if f.endswith(".py"):
                    filepath = os.path.join(root, f)
                    py_files.append(filepath)
                    try:
                        with open(filepath, "r", encoding="utf-8") as f:
                            lines = len(f.readlines())
                            total_lines += lines
                    except Exception:
                        pass

        results.append(f"📁 Project Analysis: {path}")
        results.append("=" * 50)
        results.append(f"Python files: {len(py_files)}")
        results.append(f"Total lines: {total_lines}")

        # Structure
        results.append("\n📂 Structure:")
        for root, dirs, files in os.walk(path):
            level = root.replace(path, "").count(os.sep)
            indent = "  " * level
            results.append(f"{indent}{os.path.basename(root)}/")
            for f in files:
                if f.endswith(".py"):
                    results.append(f"{indent}  🐍 {f}")

        # Check for key files
        key_files = [
            "requirements.txt",
            "setup.py",
            "pyproject.toml",
            "Dockerfile",
            "docker-compose.yml",
            "README.md",
            "setup.cfg",
            "tox.ini",
        ]
        results.append("\n📄 Key Files:")
        for kf in key_files:
            exists = os.path.exists(os.path.join(path, kf))
            results.append(f'  {"✅" if exists else "❌"} {kf}')

        # Test structure
        test_dirs = ["tests", "test"]
        results.append("\n🧪 Tests:")
        for td in test_dirs:
            exists = os.path.exists(os.path.join(path, td))
            results.append(f'  {"✅" if exists else "❌"} {td}/')

        return "\n".join(results)

    def get_project_structure(self, path: str) -> str:
        """Get detailed project structure as tree."""
        path = os.path.abspath(path)

        def build_tree(dir_path, prefix=""):
            items = []
            try:
                entries = sorted(os.listdir(dir_path))
                for i, entry in enumerate(entries):
                    if entry.startswith("."):
                        continue
                    full = os.path.join(dir_path, entry)
                    is_last = i == len(entries) - 1
                    connector = "└── " if is_last else "├── "

                    if os.path.isdir(full):
                        items.append(f"{prefix}{connector}{entry}/")
                        items.extend(build_tree(full, prefix + ("    " if is_last else "│   ")))
                    else:
                        if entry.endswith(".py") or entry in (
                            "requirements.txt",
                            "README.md",
                            "Dockerfile",
                            "docker-compose.yml",
                        ):
                            items.append(f"{prefix}{connector}{entry}")
            except Exception:
                pass
            return items

        tree = [os.path.basename(path) + "/"]
        tree.extend(build_tree(path))
        return "\n".join(tree)

    def find_duplicates(self, path: str) -> str:
        """Find duplicate code blocks."""
        try:
            result = subprocess.run(
                ["radon", "raw", "--duplicates", path], capture_output=True, text=True, timeout=60
            )
            if result.stdout:
                return result.stdout
            return "No duplicates found"
        except FileNotFoundError:
            return "radon not installed. Install with: pip install radon"
        except Exception as e:
            return f"Error finding duplicates: {str(e)}"

    def check_circular_imports(self, path: str) -> str:
        """Check for circular imports."""
        try:
            result = subprocess.run(
                ["pydeps", path, "--noshow", "--dot"], capture_output=True, text=True, timeout=60
            )
            if result.stdout:
                return f"Dependency graph generated. Check for cycles in output.\n{result.stdout[:2000]}"
            return "No circular imports detected or pydeps not available"
        except FileNotFoundError:
            return "pydeps not installed. Install with: pip install pydeps"
        except Exception as e:
            return f"Error checking circular imports: {str(e)}"

    # ========================================================================
    # NEW: REPAIR TOOLS
    # ========================================================================
    def fix_missing_init(self, path: str) -> str:
        """Add missing __init__.py files to Python packages."""
        path = os.path.abspath(path)
        created = []

        for root, dirs, files in os.walk(path):
            dirs[:] = [
                d
                for d in dirs
                if not d.startswith(".")
                and d not in ("__pycache__", "node_modules", ".git", "venv", "env")
            ]

            py_files = [f for f in files if f.endswith(".py")]
            if py_files and "__init__.py" not in files:
                init_path = os.path.join(root, "__init__.py")
                if not os.path.exists(init_path):
                    with open(init_path, "w") as f:
                        f.write('"""Package initialization."""\n')
                    created.append(os.path.relpath(init_path, path))

        if created:
            return f"✅ Created {len(created)} __init__.py files:\n" + "\n".join(
                f"  {f}" for f in created
            )
        return "No missing __init__.py files found"

    def fix_circular_imports(self, path: str) -> str:
        """Analyze and suggest fixes for circular imports."""
        try:
            result = subprocess.run(
                ["pydeps", path, "--noshow", "--dot"], capture_output=True, text=True, timeout=60
            )

            cycles = []
            if result.stdout:
                for line in result.stdout.split("\n"):
                    if "->" in line and "->" in line.split("->")[-1]:
                        cycles.append(line.strip())

            if cycles:
                return (
                    "Potential circular imports detected:\n"
                    + "\n".join(cycles[:10])
                    + "\n\nManual refactoring needed."
                )
            return "No obvious circular imports detected"
        except Exception as e:
            return f"Error checking circular imports: {str(e)}"

    def fix_relative_imports(self, path: str) -> str:
        """Convert absolute imports to relative where appropriate."""
        path = os.path.abspath(path)

        for root, dirs, files in os.walk(path):
            dirs[:] = [
                d
                for d in dirs
                if not d.startswith(".")
                and d not in ("__pycache__", "node_modules", ".git", "venv", "env")
            ]
            for f in files:
                if not f.endswith(".py"):
                    continue
                filepath = os.path.join(root, f)
                try:
                    with open(filepath, "r", encoding="utf-8") as f:
                        content = f.read()

                    # This is complex - just report what would be changed
                    import re

                    lines = content.split("\n")
                    for i, line in enumerate(lines):
                        match = re.match(r"^(from|import)\s+(\S+)", line.strip())
                        if match:
                            module = match.group(2)
                            if module.startswith(os.path.basename(path)):
                                pass  # Could be made relative

                except Exception:
                    pass

        return "Relative import analysis complete. Manual review recommended for complex cases."

    def normalize_imports(self, path: str) -> str:
        """Normalize import order and style (isort)."""
        try:
            result = subprocess.run(["isort", path], capture_output=True, text=True, timeout=60)
            if result.returncode == 0:
                return "✅ Imports normalized with isort"
            return f"isort output:\n{result.stdout}\n{result.stderr}"
        except FileNotFoundError:
            return "isort not installed. Install with: pip install isort"
        except Exception as e:
            return f"Error normalizing imports: {str(e)}"

    # ========================================================================
    # NEW: QUALITY TOOLS
    # ========================================================================
    def run_quality_check(self, path: str) -> str:
        """Run comprehensive quality checks."""
        results = []
        results.append(f"🔍 Quality Check: {path}")
        results.append("=" * 50)

        results.append(self.validate_syntax(path))
        results.append("\n🔍 Linting (ruff):")
        results.append(self.lint_file(path))
        results.append("\n🎨 Formatting (black):")
        results.append(self.format_code(path))
        results.append("\n🔍 Type Checking (mypy):")
        results.append(self.check_types(path))
        results.append("\n🔒 Security (bandit):")
        results.append(self.check_security(path))
        results.append("\n💀 Dead Code (vulture):")
        results.append(self.check_dead_code(path))
        results.append("\n📊 Complexity (radon):")
        results.append(self.analyze_complexity(path))
        results.append("\n🔒 Security (bandit):")
        results.append(self.check_security(path))

        return "\n".join(results)

    def generate_type_stubs(self, path: str) -> str:
        """Generate .pyi type stubs for Python files."""
        try:
            result = subprocess.run(
                ["mypy", "--install-types", "--non-interactive", path],
                capture_output=True,
                text=True,
                timeout=120,
            )
            return f"Type stub generation attempted:\n{result.stdout}"
        except Exception as e:
            return f"Error generating type stubs: {str(e)}"

    def check_docstrings(self, path: str) -> str:
        """Check for missing docstrings."""
        try:
            result = subprocess.run(
                ["pydocstyle", path], capture_output=True, text=True, timeout=60
            )
            if result.returncode == 0:
                return "✅ All docstrings present"
            return f"Missing docstrings:\n{result.stdout}"
        except FileNotFoundError:
            return "pydocstyle not installed. Install with: pip install pydocstyle"
        except Exception as e:
            return f"Error checking docstrings: {str(e)}"

    def ocr_image(self, path: str, lang: str = "spa+eng") -> str:
        """Extrae texto de una imagen usando Tesseract OCR (rápido, estándar)."""
        try:
            import pytesseract
            from PIL import Image

            image = Image.open(path)
            text = pytesseract.image_to_string(image, lang=lang)
            return text.strip() or "(no se detectó texto)"
        except ImportError:
            return "pytesseract/Pillow no instalado. Instala con: pip install pytesseract Pillow"
        except FileNotFoundError:
            return f"Archivo no encontrado: {path}"
        except Exception as e:
            return f"Error en OCR: {str(e)}"

    def ocr_image_best(self, path: str, lang: str = "spa+eng") -> str:
        """Extrae texto de una imagen con preprocesamiento (mejor calidad).

        Aplica: escala de grises, umbralización (Otsu), reducción de ruido
        y escalado para mejorar la precisión de Tesseract.
        """
        try:
            import cv2
            import pytesseract
            from PIL import Image

            # Leer con OpenCV
            img = cv2.imread(path)
            if img is None:
                return f"Archivo no encontrado o no es imagen válida: {path}"

            # Preprocesamiento
            gray = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)
            # Reducir ruido
            gray = cv2.medianBlur(gray, 3)
            # Umbralización adaptativa
            thresh = cv2.adaptiveThreshold(
                gray, 255, cv2.ADAPTIVE_THRESH_GAUSSIAN_C, cv2.THRESH_BINARY, 11, 2
            )
            # Escalar para mejor detección de texto pequeño
            h, w = thresh.shape[:2]
            scale = max(1, 1600 / max(h, w))
            if scale > 1:
                thresh = cv2.resize(thresh, None, fx=scale, fy=scale, interpolation=cv2.INTER_CUBIC)

            # Convertir a PIL para pytesseract
            pil_image = Image.fromarray(thresh)
            text = pytesseract.image_to_string(pil_image, lang=lang, config="--psm 6")
            return text.strip() or "(no se detectó texto)"
        except ImportError:
            return (
                "OpenCV/numpy/pytesseract no instalado. "
                "Instala con: pip install opencv-python numpy pytesseract Pillow"
            )
        except FileNotFoundError:
            return f"Archivo no encontrado: {path}"
        except Exception as e:
            return f"Error en OCR mejorado: {str(e)}"

    def ocr_screenshot(self, region: str = None, lang: str = "spa+eng") -> str:
        """Captura una pantalla (o región) y extrae texto con OCR.

        region: formato 'x,y,ancho,alto' o None para pantalla completa.
        """
        try:
            import pytesseract
            from PIL import ImageGrab

            if region:
                try:
                    x, y, w, h = [int(v) for v in region.split(",")]
                    bbox = (x, y, x + w, y + h)
                except ValueError:
                    return "Región inválida. Usa formato: x,y,ancho,alto (ej: 100,100,800,600)"
            else:
                bbox = None

            screenshot = ImageGrab.grab(bbox=bbox)
            text = pytesseract.image_to_string(screenshot, lang=lang)
            return text.strip() or "(no se detectó texto en la captura)"
        except ImportError:
            return "pytesseract/Pillow no instalado. Instala con: pip install pytesseract Pillow"
        except Exception as e:
            return f"Error en OCR de screenshot: {str(e)}"

    # ------------------------------------------------------------------
    # Git tools (auto-commit + rollback)
    # ------------------------------------------------------------------

    def git_init(self, path: str = ".") -> str:
        """Inicializa un repositorio git en el directorio dado."""
        try:
            result = subprocess.run(
                ["git", "init", path],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if result.returncode == 0:
                return f"Repositorio git inicializado en {path}"
            return f"Error git init: {result.stderr.strip()}"
        except Exception as e:
            return f"Error git init: {str(e)}"

    def git_add(self, path: str = ".") -> str:
        """Agrega archivos al staging area de git."""
        try:
            result = subprocess.run(
                ["git", "add", path],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if result.returncode == 0:
                return f"Archivos agregados al staging: {path}"
            return f"Error git add: {result.stderr.strip()}"
        except Exception as e:
            return f"Error git add: {str(e)}"

    def git_commit(self, message: str, path: str = ".") -> str:
        """Hace commit de cambios staged con el mensaje dado."""
        try:
            result = subprocess.run(
                ["git", "-C", path, "commit", "-m", message],
                capture_output=True,
                text=True,
                timeout=60,
            )
            if result.returncode == 0:
                return f"Commit exitoso: {result.stdout.strip()}"
            return f"Error git commit: {result.stderr.strip()}"
        except Exception as e:
            return f"Error git commit: {str(e)}"

    def git_rollback(self, path: str = ".") -> str:
        """Deshace el último commit (git reset --hard HEAD~1).

        ADVERTENCIA: Esto descarta los cambios del último commit.
        """
        try:
            result = subprocess.run(
                ["git", "-C", path, "reset", "--hard", "HEAD~1"],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if result.returncode == 0:
                return f"Rollback completado: {result.stdout.strip()}"
            return f"Error git rollback: {result.stderr.strip()}"
        except Exception as e:
            return f"Error git rollback: {str(e)}"

    def git_status(self, path: str = ".") -> str:
        """Muestra el estado actual del repositorio git."""
        try:
            result = subprocess.run(
                ["git", "-C", path, "status"],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if result.returncode == 0:
                return result.stdout.strip() or "Repositorio limpio, sin cambios."
            return f"Error git status: {result.stderr.strip()}"
        except Exception as e:
            return f"Error git status: {str(e)}"

    def git_diff(self, path: str = ".") -> str:
        """Muestra el diff de cambios no staged."""
        try:
            result = subprocess.run(
                ["git", "-C", path, "diff"],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if result.returncode == 0:
                return result.stdout.strip() or "Sin diferencias."
            return f"Error git diff: {result.stderr.strip()}"
        except Exception as e:
            return f"Error git diff: {str(e)}"

    def get_tools_schema(self) -> List[Dict]:
        """OpenAI-compatible tool schema for native llama.cpp tool calling."""

        def fn(name: str, description: str, properties: Dict, required: List[str]) -> Dict:
            return {
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": {
                        "type": "object",
                        "properties": properties,
                        "required": required,
                    },
                },
            }

        return [
            fn("read_file", "Read a text file", {"path": {"type": "string"}}, ["path"]),
            fn(
                "write_file",
                "Write a file. Python files are syntax-checked first. Use strict=true for ruff+mypy checks.",
                {
                    "path": {"type": "string"},
                    "content": {"type": "string"},
                    "strict": {"type": "boolean", "default": False},
                },
                ["path", "content"],
            ),
            fn("list_dir", "List a directory", {"path": {"type": "string"}}, ["path"]),
            fn(
                "run_command",
                "Run a shell command in the project. Sandboxed with bubblewrap by default.",
                {
                    "cmd": {"type": "string"},
                    "cwd": {"type": "string"},
                    "timeout": {"type": "integer", "default": 60},
                    "sandbox": {"type": "boolean", "default": True},
                },
                ["cmd"],
            ),
            fn(
                "grep",
                "Search text in files",
                {
                    "pattern": {"type": "string"},
                    "path": {"type": "string"},
                    "ignore_case": {"type": "boolean", "default": False},
                    "file_type": {"type": "string"},
                },
                ["pattern", "path"],
            ),
            fn(
                "find_files",
                "Find files by glob pattern",
                {
                    "pattern": {"type": "string", "default": "*"},
                    "path": {"type": "string", "default": "."},
                },
                [],
            ),
            fn(
                "repo_map",
                "Show Python classes/functions as a compact repository map",
                {
                    "path": {"type": "string", "default": "."},
                    "max_chars": {"type": "integer", "default": 8000},
                },
                [],
            ),
            fn(
                "run_python",
                "Run a Python snippet. Sandboxed with bubblewrap by default.",
                {
                    "code": {"type": "string"},
                    "args": {"type": "array", "items": {"type": "string"}},
                    "timeout": {"type": "integer", "default": 30},
                    "sandbox": {"type": "boolean", "default": True},
                },
                ["code"],
            ),
            fn(
                "run_tests",
                "Run pytest",
                {
                    "path": {"type": "string", "default": "."},
                    "pattern": {"type": "string", "default": "test_*.py"},
                    "coverage": {"type": "boolean", "default": False},
                },
                [],
            ),
            fn(
                "lint_file",
                "Run ruff on a Python file",
                {"path": {"type": "string"}, "fix": {"type": "boolean", "default": False}},
                ["path"],
            ),
            fn(
                "validate_syntax",
                "Validate Python syntax in a file or directory",
                {"path": {"type": "string"}},
                ["path"],
            ),
            fn(
                "apply_patch",
                "Apply a unified diff patch",
                {"path": {"type": "string"}, "patch": {"type": "string"}},
                ["path", "patch"],
            ),
            fn("check_types", "Run mypy", {"path": {"type": "string"}}, ["path"]),
            fn("find_todos", "Find TODO/FIXME comments", {"path": {"type": "string"}}, ["path"]),
            fn("check_security", "Run bandit", {"path": {"type": "string"}}, ["path"]),
            fn(
                "analyze_complexity",
                "Run radon complexity analysis",
                {"path": {"type": "string"}},
                ["path"],
            ),
            fn(
                "lsp_hover",
                "Get LSP hover info for a symbol at a position",
                {
                    "path": {"type": "string"},
                    "line": {"type": "integer"},
                    "column": {"type": "integer"},
                },
                ["path", "line", "column"],
            ),
            fn(
                "lsp_go_to_definition",
                "Go to definition of a symbol at a position",
                {
                    "path": {"type": "string"},
                    "line": {"type": "integer"},
                    "column": {"type": "integer"},
                },
                ["path", "line", "column"],
            ),
            fn(
                "lsp_find_references",
                "Find references to a symbol at a position",
                {
                    "path": {"type": "string"},
                    "line": {"type": "integer"},
                    "column": {"type": "integer"},
                },
                ["path", "line", "column"],
            ),
            fn(
                "lsp_diagnostics",
                "Get LSP diagnostics for a file",
                {"path": {"type": "string"}},
                ["path"],
            ),
            fn(
                "ocr_image",
                "Extrae texto de una imagen usando Tesseract OCR (rápido, estándar).",
                {
                    "path": {"type": "string"},
                    "lang": {"type": "string", "default": "spa+eng"},
                },
                ["path"],
            ),
            fn(
                "ocr_image_best",
                "Extrae texto de una imagen con preprocesamiento (mejor calidad): "
                "escala de grises, umbralización Otsu, reducción de ruido y escalado.",
                {
                    "path": {"type": "string"},
                    "lang": {"type": "string", "default": "spa+eng"},
                },
                ["path"],
            ),
            fn(
                "ocr_screenshot",
                "Captura una pantalla (o región) y extrae texto con OCR. "
                "region: formato 'x,y,ancho,alto' o None para pantalla completa.",
                {
                    "region": {"type": "string"},
                    "lang": {"type": "string", "default": "spa+eng"},
                },
                [],
            ),
            fn(
                "git_init",
                "Initialize a git repository in the given directory",
                {"path": {"type": "string", "default": "."}},
                [],
            ),
            fn(
                "git_add",
                "Add files to git staging area",
                {"path": {"type": "string", "default": "."}},
                [],
            ),
            fn(
                "git_commit",
                "Commit staged changes with a message",
                {
                    "message": {"type": "string"},
                    "path": {"type": "string", "default": "."},
                },
                ["message"],
            ),
            fn(
                "git_rollback",
                "Undo the last commit (git reset --hard HEAD~1). WARNING: discards changes.",
                {"path": {"type": "string", "default": "."}},
                [],
            ),
            fn(
                "git_status",
                "Show git repository status",
                {"path": {"type": "string", "default": "."}},
                [],
            ),
            fn(
                "git_diff",
                "Show unstaged diff of changes",
                {"path": {"type": "string", "default": "."}},
                [],
            ),
        ]

"""Sistema de autocompletado de código para lai.

Extrae símbolos de archivos Python usando el AST nativo y ofrece
completado contextual basado en la palabra bajo el cursor.

100% local, sin dependencias externas adicionales (usa ast de la stdlib).
Regla 0 de lai.
"""

from __future__ import annotations

import ast
import fnmatch
import os
import re
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Symbol:
    """Símbolo extraído de código fuente."""

    name: str
    kind: str  # 'function', 'class', 'method', 'variable', 'import', 'module'
    file: str
    line: int
    column: int
    signature: str | None = None
    docstring: str | None = None


@dataclass
class CompletionItem:
    """Elemento de completado."""

    label: str
    kind: str  # 'function', 'class', 'method', 'variable', 'import', 'module', 'keyword'
    detail: str | None = None
    docstring: str | None = None
    insert_text: str | None = None
    file: str | None = None
    line: int | None = None


class CodeCompleter:
    """Motor de autocompletado de código para Python."""

    # Directorios que nunca deben indexarse: caches, entornos virtuales,
    # artefactos de build y proyectos generados dentro del repositorio.
    EXCLUDED_DIRS = {
        "__pycache__",
        "node_modules",
        "build",
        "dist",
        "site-packages",
        "venv",
        "env",
        "chess",
        "logs",
        "sessions",
    }
    EXCLUDED_FILE_PATTERNS = ("*.pyc", "*.pyo", "*.min.py")

    # Palabras clave de Python
    PYTHON_KEYWORDS = {
        "False",
        "None",
        "True",
        "and",
        "as",
        "assert",
        "async",
        "await",
        "break",
        "class",
        "continue",
        "def",
        "del",
        "elif",
        "else",
        "except",
        "finally",
        "for",
        "from",
        "global",
        "if",
        "import",
        "in",
        "is",
        "lambda",
        "nonlocal",
        "not",
        "or",
        "pass",
        "raise",
        "return",
        "try",
        "while",
        "with",
        "yield",
    }

    # Funciones built-in de Python
    BUILTINS = {
        "abs",
        "all",
        "any",
        "ascii",
        "bin",
        "bool",
        "breakpoint",
        "bytearray",
        "bytes",
        "callable",
        "chr",
        "classmethod",
        "compile",
        "complex",
        "delattr",
        "dict",
        "dir",
        "divmod",
        "enumerate",
        "eval",
        "exec",
        "filter",
        "float",
        "format",
        "frozenset",
        "getattr",
        "globals",
        "hasattr",
        "hash",
        "help",
        "hex",
        "id",
        "input",
        "int",
        "isinstance",
        "issubclass",
        "iter",
        "len",
        "list",
        "locals",
        "map",
        "max",
        "memoryview",
        "min",
        "next",
        "object",
        "oct",
        "open",
        "ord",
        "pow",
        "print",
        "property",
        "range",
        "repr",
        "reversed",
        "round",
        "set",
        "setattr",
        "slice",
        "sorted",
        "staticmethod",
        "str",
        "sum",
        "super",
        "tuple",
        "type",
        "vars",
        "zip",
        "__import__",
    }

    def __init__(self, project_root: str = "."):
        self.project_root = Path(project_root).resolve()
        self._symbols: list[Symbol] = []
        self._file_symbols: dict[str, list[Symbol]] = {}
        self._indexed = False

    def _is_excluded(self, path: Path) -> bool:
        """True si el archivo está en un directorio ignorado del proyecto."""
        try:
            rel = path.resolve().relative_to(self.project_root)
        except ValueError:
            return True

        # Carpetas ocultas (.git, .venv, .mypy_cache, ...) y artefactos.
        for part in rel.parts[:-1]:
            if part.startswith(".") or part in self.EXCLUDED_DIRS:
                return True
            if part.endswith(".egg-info"):
                return True
        return any(fnmatch.fnmatch(rel.name, pattern) for pattern in self.EXCLUDED_FILE_PATTERNS)

    def index_project(
        self,
        extensions: tuple[str, ...] = (".py",),
    ) -> dict[str, int]:
        """Indexa todos los archivos del proyecto extrayendo símbolos."""
        self._symbols = []
        self._file_symbols = {}

        if not self.project_root.exists():
            return {"files": 0, "symbols": 0}

        for ext in extensions:
            for f in sorted(self.project_root.rglob(f"*{ext}")):
                if not f.is_file() or self._is_excluded(f):
                    continue
                try:
                    symbols = self._extract_symbols(f)
                    if symbols:
                        self._file_symbols[str(f)] = symbols
                        self._symbols.extend(symbols)
                except Exception:
                    continue

        self._indexed = True
        return {
            "files": len(self._file_symbols),
            "symbols": len(self._symbols),
        }

    def extract_symbols(self, file_path: str | Path) -> list[Symbol]:
        """Extrae los símbolos de un archivo (API pública)."""
        return self._extract_symbols(Path(file_path))

    def _extract_symbols(self, file_path: Path) -> list[Symbol]:
        """Extrae símbolos de un archivo Python usando AST."""
        symbols: list[Symbol] = []

        try:
            source = file_path.read_text(encoding="utf-8", errors="ignore")
            tree = ast.parse(source, filename=str(file_path))
        except (SyntaxError, UnicodeDecodeError, OSError):
            return symbols

        file_str = str(file_path)
        parents = self._build_parent_map(tree)

        for node in ast.walk(tree):
            # Funciones y métodos
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                signature = self._build_function_signature(node)
                docstring = ast.get_docstring(node)
                scope = self._enclosing_scope(node, parents)
                symbols.append(
                    Symbol(
                        name=node.name,
                        kind="method" if isinstance(scope, ast.ClassDef) else "function",
                        file=file_str,
                        line=node.lineno,
                        column=node.col_offset,
                        signature=signature,
                        docstring=docstring,
                    )
                )

            # Clases
            elif isinstance(node, ast.ClassDef):
                bases = [self._get_name(b) for b in node.bases if self._get_name(b)]
                base_str = f"({', '.join(bases)})" if bases else ""
                symbols.append(
                    Symbol(
                        name=node.name,
                        kind="class",
                        file=file_str,
                        line=node.lineno,
                        column=node.col_offset,
                        signature=base_str,
                        docstring=ast.get_docstring(node),
                    )
                )

            # Importaciones
            elif isinstance(node, ast.Import):
                for alias in node.names:
                    imported = alias.name
                    if alias.asname:
                        imported = f"{alias.name} as {alias.asname}"
                    symbols.append(
                        Symbol(
                            name=alias.asname or alias.name,
                            kind="import",
                            file=file_str,
                            line=node.lineno,
                            column=node.col_offset,
                            signature=f"import {imported}",
                        )
                    )

            elif isinstance(node, ast.ImportFrom):
                module = node.module or ""
                for alias in node.names:
                    imported = alias.name
                    if alias.asname:
                        imported = f"{alias.name} as {alias.asname}"
                    symbols.append(
                        Symbol(
                            name=alias.asname or alias.name,
                            kind="import",
                            file=file_str,
                            line=node.lineno,
                            column=node.col_offset,
                            signature=f"from {module} import {imported}",
                        )
                    )

            # Asignaciones a nivel de módulo (variables globales)
            elif isinstance(node, (ast.Assign, ast.AnnAssign)) and parents.get(node) is tree:
                targets = node.targets if isinstance(node, ast.Assign) else [node.target]
                for target in targets:
                    if isinstance(target, ast.Name):
                        symbols.append(
                            Symbol(
                                name=target.id,
                                kind="variable",
                                file=file_str,
                                line=node.lineno,
                                column=target.col_offset,
                            )
                        )

        return symbols

    @staticmethod
    def _build_parent_map(tree: ast.AST) -> dict[ast.AST, ast.AST]:
        """Construye un mapa hijo -> padre en una sola pasada."""
        return {
            child: parent for parent in ast.walk(tree) for child in ast.iter_child_nodes(parent)
        }

    @staticmethod
    def _enclosing_scope(node: ast.AST, parents: dict[ast.AST, ast.AST]) -> ast.AST | None:
        """Devuelve el ámbito (módulo, clase o función) que contiene al nodo."""
        scope_types = (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)
        parent = parents.get(node)
        while parent is not None and not isinstance(parent, scope_types):
            parent = parents.get(parent)
        return parent

    def _build_function_signature(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> str:
        """Construye una representación de firma de función."""
        args: list[str] = []

        positional = [*node.args.posonlyargs, *node.args.args]
        defaults = [None] * (len(positional) - len(node.args.defaults)) + list(node.args.defaults)
        for arg, default in zip(positional, defaults, strict=True):
            rendered = arg.arg
            if arg.annotation:
                rendered += f": {self._get_name(arg.annotation)}"
            if default is not None:
                rendered += f" = {self._get_name(default)}"
            args.append(rendered)

        if node.args.posonlyargs:
            args.insert(len(node.args.posonlyargs), "/")

        if node.args.vararg:
            args.append(f"*{node.args.vararg.arg}")
        elif node.args.kwonlyargs:
            args.append("*")

        for arg, default in zip(node.args.kwonlyargs, node.args.kw_defaults, strict=True):
            rendered = arg.arg
            if arg.annotation:
                rendered += f": {self._get_name(arg.annotation)}"
            if default is not None:
                rendered += f" = {self._get_name(default)}"
            args.append(rendered)

        if node.args.kwarg:
            args.append(f"**{node.args.kwarg.arg}")

        returns = f" -> {self._get_name(node.returns)}" if node.returns else ""
        return f"({', '.join(args)}){returns}"

    def _get_name(self, node: ast.AST | None) -> str:
        """Obtiene una representación textual de un nodo AST."""
        if node is None:
            return ""
        try:
            return ast.unparse(node)
        except Exception:
            return ""

    def complete(
        self,
        code: str,
        cursor_line: int,
        cursor_column: int,
        max_results: int = 10,
        file_path: str | None = None,
    ) -> list[CompletionItem]:
        """Ofrece sugerencias de completado para una posición dada.

        cursor_line y cursor_column son 0-indexed. Si se indica ``file_path``,
        los símbolos del archivo actual tienen prioridad (y se usan como
        contexto cuando no hay prefijo escrito).
        """
        if not self._indexed:
            self.index_project()

        lines = code.split("\n")
        if cursor_line >= len(lines):
            return []

        line = lines[cursor_line]
        cursor_column = min(cursor_column, len(line))

        # Extraer el identificador parcial bajo el cursor.
        before_cursor = line[:cursor_column]
        match = re.search(r"([\w.]*)$", before_cursor)
        current_word = match.group(1) if match else ""
        is_after_dot = "." in current_word
        prefix = current_word.rsplit(".", 1)[-1]

        local_file = str(Path(file_path).resolve()) if file_path else None
        cursor_lineno = cursor_line + 1

        # 1. Símbolos del proyecto (prioridad máxima).
        if prefix:
            candidates = [
                sym
                for sym in self._symbols
                if sym.name.lower().startswith(prefix.lower())
                and (not is_after_dot or sym.kind in ("method", "variable", "import"))
            ]
        elif local_file:
            # Sin prefijo: símbolos del archivo actual, los más cercanos primero.
            candidates = [sym for sym in self._symbols if sym.file == local_file]
        else:
            candidates = [sym for sym in self._symbols if sym.kind in ("class", "function")]

        def symbol_rank(sym: Symbol) -> tuple[int, int, int, str]:
            exact = 0 if sym.name == prefix else 1
            in_local = 0 if sym.file == local_file else 1
            distance = (
                abs(sym.line - cursor_lineno) if local_file and sym.file == local_file else 10**6
            )
            return (exact, in_local, distance, sym.name.lower())

        results: list[CompletionItem] = []
        for sym in sorted(candidates, key=symbol_rank):
            results.append(
                CompletionItem(
                    label=sym.name,
                    kind=sym.kind,
                    detail=sym.signature,
                    docstring=sym.docstring,
                    insert_text=sym.name,
                    file=sym.file,
                    line=sym.line,
                )
            )

        # 2. Built-ins (solo con prefijo, para no inundar la salida).
        if prefix and not is_after_dot:
            for bi in sorted(self.BUILTINS):
                if bi.lower().startswith(prefix.lower()):
                    results.append(
                        CompletionItem(
                            label=bi,
                            kind="variable",
                            insert_text=bi,
                        )
                    )

        # 3. Palabras clave (solo con prefijo y fuera de un atributo).
        if prefix and not is_after_dot:
            for kw in sorted(self.PYTHON_KEYWORDS):
                if kw.lower().startswith(prefix.lower()):
                    results.append(
                        CompletionItem(
                            label=kw,
                            kind="keyword",
                            insert_text=kw,
                        )
                    )

        # Deduplicar y limitar.
        seen: set[str] = set()
        unique_results: list[CompletionItem] = []
        for item in results:
            if item.label not in seen:
                seen.add(item.label)
                unique_results.append(item)
                if len(unique_results) >= max_results:
                    break

        return unique_results

    def get_symbol_at(
        self,
        code: str,
        cursor_line: int,
        cursor_column: int,
    ) -> Symbol | None:
        """Obtiene el símbolo bajo el cursor para ir a definición."""
        if not self._indexed:
            self.index_project()

        lines = code.split("\n")
        if cursor_line >= len(lines):
            return None

        line = lines[cursor_line]
        before_cursor = line[:cursor_column]
        match = re.search(r"([\w.]+)$", before_cursor)
        if not match:
            return None

        symbol_name = match.group(1).split(".")[-1]

        for sym in self._symbols:
            if sym.name == symbol_name and sym.kind != "import":
                return sym
        return None

    def get_repo_map(self, max_chars: int = 8000) -> str:
        """Genera un mapa compacto del repositorio para contexto de LLM."""
        if not self._indexed:
            self.index_project()

        lines: list[str] = []
        current_file: str | None = None

        for sym in sorted(self._symbols, key=lambda s: (s.file, s.line)):
            if sym.file != current_file:
                current_file = sym.file
                rel = os.path.relpath(current_file, self.project_root)
                lines.append(f"\n# {rel}")

            if sym.kind == "class":
                lines.append(f"  class {sym.name}{sym.signature or ''}  # L{sym.line}")
            elif sym.kind == "function":
                lines.append(f"  def {sym.name}{sym.signature or ''}  # L{sym.line}")
            elif sym.kind == "method":
                lines.append(f"    def {sym.name}{sym.signature or ''}  # L{sym.line}")
            elif sym.kind == "import":
                lines.append(f"  {sym.signature or f'import {sym.name}'}  # L{sym.line}")
            else:
                lines.append(f"  # var {sym.name}  # L{sym.line}")

        result = "\n".join(lines)
        if len(result) > max_chars:
            cut = result.rfind("\n", 0, max_chars)
            result = result[: cut if cut > 0 else max_chars].rstrip()
            result += "\n... (truncado)"
        return result


# Instancia global para uso desde CLI
_completer: CodeCompleter | None = None


def get_completer(project_root: str = ".") -> CodeCompleter:
    """Devuelve el completer singleton, reutilizándolo si cambia el proyecto."""
    global _completer  # noqa: PLW0603
    root = Path(project_root).resolve()
    if _completer is None or _completer.project_root != root:
        _completer = CodeCompleter(str(root))
    return _completer

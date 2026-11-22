"""Sistema de autocompletado de código para lai.

Extrae símbolos de archivos Python usando el AST nativo y ofrece
completado contextual basado en la palabra bajo el cursor.

100% local, sin dependencias externas adicionales (usa ast de la stdlib).
Regla 0 de lai.
"""

from __future__ import annotations

import ast
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

    # Palabras clave de Python
    PYTHON_KEYWORDS = {
        'False', 'None', 'True', 'and', 'as', 'assert', 'async', 'await',
        'break', 'class', 'continue', 'def', 'del', 'elif', 'else', 'except',
        'finally', 'for', 'from', 'global', 'if', 'import', 'in', 'is',
        'lambda', 'nonlocal', 'not', 'or', 'pass', 'raise', 'return', 'try',
        'while', 'with', 'yield',
    }

    # Funciones built-in de Python
    BUILTINS = {
        'abs', 'all', 'any', 'ascii', 'bin', 'bool', 'breakpoint', 'bytearray',
        'bytes', 'callable', 'chr', 'classmethod', 'compile', 'complex',
        'delattr', 'dict', 'dir', 'divmod', 'enumerate', 'eval', 'exec',
        'filter', 'float', 'format', 'frozenset', 'getattr', 'globals',
        'hasattr', 'hash', 'help', 'hex', 'id', 'input', 'int', 'isinstance',
        'issubclass', 'iter', 'len', 'list', 'locals', 'map', 'max', 'memoryview',
        'min', 'next', 'object', 'oct', 'open', 'ord', 'pow', 'print', 'property',
        'range', 'repr', 'reversed', 'round', 'set', 'setattr', 'slice', 'sorted',
        'staticmethod', 'str', 'sum', 'super', 'tuple', 'type', 'vars', 'zip',
        '__import__',
    }

    def __init__(self, project_root: str = '.'):
        self.project_root = Path(project_root).resolve()
        self._symbols: list[Symbol] = []
        self._file_symbols: dict[str, list[Symbol]] = {}
        self._indexed = False

    def index_project(
        self,
        extensions: tuple[str, ...] = ('.py',),
    ) -> dict[str, int]:
        """Indexa todos los archivos del proyecto extrayendo símbolos."""
        self._symbols = []
        self._file_symbols = {}

        if not self.project_root.exists():
            return {'files': 0, 'symbols': 0}

        for ext in extensions:
            for f in self.project_root.rglob(f'*{ext}'):
                try:
                    symbols = self._extract_symbols(f)
                    if symbols:
                        self._file_symbols[str(f)] = symbols
                        self._symbols.extend(symbols)
                except Exception:
                    continue

        self._indexed = True
        return {
            'files': len(self._file_symbols),
            'symbols': len(self._symbols),
        }

    def _extract_symbols(self, file_path: Path) -> list[Symbol]:
        """Extrae símbolos de un archivo Python usando AST."""
        symbols: list[Symbol] = []

        try:
            source = file_path.read_text(encoding='utf-8', errors='ignore')
            tree = ast.parse(source, filename=str(file_path))
        except (SyntaxError, UnicodeDecodeError, OSError):
            return symbols

        file_str = str(file_path)

        for node in ast.walk(tree):
            # Funciones y métodos
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                signature = self._build_function_signature(node)
                docstring = ast.get_docstring(node)
                symbols.append(Symbol(
                    name=node.name,
                    kind='method' if self._is_inside_class(node, tree) else 'function',
                    file=file_str,
                    line=node.lineno,
                    column=node.col_offset,
                    signature=signature,
                    docstring=docstring,
                ))

            # Clases
            elif isinstance(node, ast.ClassDef):
                bases = [self._get_name(b) for b in node.bases]
                base_str = f"({', '.join(bases)})" if bases else ''
                symbols.append(Symbol(
                    name=node.name,
                    kind='class',
                    file=file_str,
                    line=node.lineno,
                    column=node.col_offset,
                    signature=base_str,
                    docstring=ast.get_docstring(node),
                ))

            # Importaciones
            elif isinstance(node, ast.Import):
                for alias in node.names:
                    symbols.append(Symbol(
                        name=alias.asname or alias.name,
                        kind='import',
                        file=file_str,
                        line=node.lineno,
                        column=node.col_offset,
                        signature=alias.name,
                    ))

            elif isinstance(node, ast.ImportFrom):
                module = node.module or ''
                for alias in node.names:
                    symbols.append(Symbol(
                        name=alias.asname or alias.name,
                        kind='import',
                        file=file_str,
                        line=node.lineno,
                        column=node.col_offset,
                        signature=f"from {module} import {alias.name}",
                    ))

            # Asignaciones a nivel de módulo (variables globales)
            elif isinstance(node, ast.Assign) and self._is_module_level(node, tree):
                for target in node.targets:
                    if isinstance(target, ast.Name):
                        symbols.append(Symbol(
                            name=target.id,
                            kind='variable',
                            file=file_str,
                            line=node.lineno,
                            column=target.col_offset,
                        ))

        return symbols

    def _is_inside_class(self, node: ast.FunctionDef, tree: ast.AST) -> bool:
        """Determina si una función está dentro de una clase."""
        for n in ast.walk(tree):
            if isinstance(n, ast.ClassDef):
                for child in ast.walk(n):
                    if child is node:
                        return True
        return False

    def _is_module_level(self, node: ast.Assign, tree: ast.AST) -> bool:
        """Determina si una asignación está a nivel de módulo."""
        for parent in ast.walk(tree):
            for child in ast.iter_child_nodes(parent):
                if child is node:
                    return isinstance(parent, ast.Module)
        return False

    def _build_function_signature(self, node: ast.FunctionDef) -> str:
        """Construye una representación de firma de función."""
        args = []
        for arg in node.args.args:
            annotation = f": {self._get_name(arg.annotation)}" if arg.annotation else ""
            args.append(f"{arg.arg}{annotation}")
        if node.args.vararg:
            args.append(f"*{node.args.vararg.arg}")
        for arg in node.args.kwonlyargs:
            annotation = f": {self._get_name(arg.annotation)}" if arg.annotation else ""
            args.append(f"{arg.arg}{annotation}")
        if node.args.kwarg:
            args.append(f"**{node.args.kwarg.arg}")
        returns = f" -> {self._get_name(node.returns)}" if node.returns else ""
        return f"({', '.join(args)}){returns}"

    def _get_name(self, node: ast.AST | None) -> str:
        """Obtiene el nombre de un nodo AST."""
        if node is None:
            return ''
        if isinstance(node, ast.Name):
            return node.id
        if isinstance(node, ast.Attribute):
            return f"{self._get_name(node.value)}.{node.attr}"
        if isinstance(node, ast.Constant):
            return repr(node.value)
        return ''

    def complete(  # noqa: PLR0912
        self,
        code: str,
        cursor_line: int,
        cursor_column: int,
        max_results: int = 10,
    ) -> list[CompletionItem]:
        """Ofrece sugerencias de completado para una posición dada.

        cursor_line y cursor_column son 0-indexed.
        """
        if not self._indexed:
            self.index_project()

        # Obtener la palabra bajo el cursor
        lines = code.split('\n')
        if cursor_line >= len(lines):
            return []

        line = lines[cursor_line]
        cursor_column = min(cursor_column, len(line))

        # Extraer la palabra actual (identificador parcial)
        before_cursor = line[:cursor_column]
        match = re.search(r'([\w.]*)$', before_cursor)
        current_word = match.group(1) if match else ''

        # Determinar el contexto (después de punto, después de import, etc.)
        is_after_dot = '.' in current_word
        is_start_of_line = before_cursor.strip() == '' or before_cursor.strip() == current_word

        results: list[CompletionItem] = []

        # 1. Palabras clave de Python
        if not is_after_dot or is_start_of_line:
            for kw in sorted(self.PYTHON_KEYWORDS):
                if current_word.lower() in kw.lower() or kw.startswith(current_word):
                    results.append(CompletionItem(
                        label=kw,
                        kind='keyword',
                        insert_text=kw,
                    ))

        # 2. Built-ins
        if not is_after_dot or is_start_of_line:
            for bi in sorted(self.BUILTINS):
                if current_word.lower() in bi.lower() or bi.startswith(current_word):
                    results.append(CompletionItem(
                        label=bi,
                        kind='variable',
                        insert_text=bi,
                    ))

        # 3. Símbolos del proyecto
        prefix = current_word.split('.')[-1] if is_after_dot else current_word
        for sym in self._symbols:
            if sym.name.startswith(prefix) or prefix in sym.name:
                kind_label = sym.kind
                if is_after_dot and sym.kind in ('method', 'variable'):
                    kind_label = sym.kind
                results.append(CompletionItem(
                    label=sym.name,
                    kind=kind_label,
                    detail=sym.signature,
                    docstring=sym.docstring,
                    insert_text=sym.name,
                    file=sym.file,
                    line=sym.line,
                ))

        # 4. Si estamos después de punto, sugerir métodos/atributos de la clase/variable
        if is_after_dot:
            base_name = current_word.rsplit('.', 1)[0]
            for sym in self._symbols:
                # Buscar métodos de clases o atributos
                if sym.kind in ('method', 'variable') and sym.name.startswith(prefix):
                    results.append(CompletionItem(
                        label=f"{base_name}.{sym.name}",
                        kind=sym.kind,
                        detail=sym.signature,
                        insert_text=sym.name,
                    ))

        # Deduplicar y limitar
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
        lines = code.split('\n')
        if cursor_line >= len(lines):
            return None

        line = lines[cursor_line]
        before_cursor = line[:cursor_column]
        match = re.search(r'([\w.]+)$', before_cursor)
        if not match:
            return None

        symbol_name = match.group(1).split('.')[-1]

        for sym in self._symbols:
            if sym.name == symbol_name:
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
            kind_prefix = {
                'class': 'class ',
                'function': 'def ',
                'method': '    def ',
                'import': '# import ',
                'variable': '# var ',
            }.get(sym.kind, '')
            sig = f" {sym.signature}" if sym.signature else ""
            lines.append(f"  {kind_prefix}{sym.name}{sig}  # L{sym.line}")

        result = '\n'.join(lines)
        if len(result) > max_chars:
            result = result[:max_chars] + '\n... (truncado)'
        return result


# Instancia global para uso desde CLI
_completer: CodeCompleter | None = None


def get_completer(project_root: str = '.') -> CodeCompleter:
    """Devuelve el completer singleton."""
    global _completer  # noqa: PLW0603
    if _completer is None:
        _completer = CodeCompleter(project_root)
    return _completer

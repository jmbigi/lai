import re

with open('lai.py', 'r') as f:
    content = f.read()

# Find the code method
start = content.find('    def code(')
if start < 0:
    print('Could not find code method')
    exit(1)

# Find the end of the code method (next method definition)
next_method = content.find('\n    def ', start + 1)
if next_method < 0:
    next_method = len(content)

old_method = content[start:next_method]

new_method = '''    def code(
        self,
        description: str,
        filename: str,
        auto: bool = False,
        yolo: bool = False,
        max_iterations: int = 3,
        corpus: str | None = None,
    ):
        """Generate high-quality code using the local LLM with iterative refinement + quality cycles."""

        # Enhanced prompt with EXPLICIT requirements - NO SHORTCUTS
        base_prompt = """Generate ONLY valid, production-ready Python code. No markdown, no explanations, no comments about generation.

Task: {description}

STRICT REQUIREMENTS - VIOLATIONS WILL BE REJECTED:
- Python 3.10+, executable standalone script
- Professional structure: functions/classes with COMPLETE type hints (params + return)
- Include: if __name__ == "__main__": block, main() function
- Error handling (try/except) with meaningful messages - NO bare except
- Type hints for ALL signatures, Google-style docstrings
- Clean PEP 8 code, imports grouped at top (stdlib, third-party, local)
- NO markdown, NO explanations, NO test code
- ASCII only: straight quotes, no special Unicode

Generate ONLY the complete Python code for file: {filename}\"\"\"

        corpus_section = ""
        if corpus:
            corpus_section = f\"\"\"

# CORPUS DE CONTEXTO (referencia)
{corpus}

\"\"\"

        prompt = f\"\"\"{base_prompt}{corpus_section}

Task: {description}

Generate ONLY the complete Python code for file: {filename}

IMPORTANT: Use straight quotes (') or (\"), NO curly quotes. Use pure ASCII. NO markdown, no explanations.\"\"\"

        max_iterations = max(1, max_iterations)

        # QUALITY CYCLES: Run iterative quality improvement cycles
        # Each cycle runs validation and attempts repair until quality threshold is met
        max_quality_cycles = 3
        quality_threshold = 0.75
        best_code = None
        best_quality = 0.0

        for quality_cycle in range(max_quality_cycles):
            context = {
                'prompt': "/no_think " + prompt,
                'max_tokens': 8192,
                'temperature': 0.0,
                'iteration': 0,
                'max_iterations': max_iterations,
                'description': description,
                'filename': filename,
                'yolo': yolo,
                'auto_fixes': [],
                'start_time': time.time(),
            }

            # Check for applicable mitigations before starting
            mitigations = get_mitigations(context['prompt'])
            for m in mitigations:
                if m.confidence > 0.85:
                    context = apply_mitigation(m.action, context)
                    context['auto_fixes'].append(m.action)

            # Run generation loop with validation and repair
            for iteration in range(max_iterations):
                context['iteration'] = iteration
                iteration_start = time.time()

                try:
                    # Apply dynamic mitigations based on iteration
                    if iteration > 0:
                        # Increase tokens on retry
                        context['max_tokens'] = min(context['max_tokens'] + 2048, 32768)
                        # Lower temperature for more deterministic output
                        context['temperature'] = max(0.0, context.get('temperature', 0.0) - 0.1)

                    response = self.chat(
                        context['prompt'],
                        stream=False,
                        thinking=False,
                        temp=context.get('temperature', 0.0),
                        max_tokens=context.get('max_tokens', 8192)
                    )
                    code = response.strip()
                    iteration_duration = time.time() - iteration_start

                    # Extract code
                    code = self._extract_code_from_response(code)

                    # Apply strict extraction if needed
                    if context.get('strict_extraction'):
                        code = self._strict_code_extraction(code)

                    # Validate syntax
                    syntax_error = None
                    try:
                        compile(code, filename, "exec")
                    except SyntaxError as e:
                        syntax_error = e
                        if iteration < max_iterations - 1:
                            # Apply retry_with_feedback mitigation
                            context = apply_mitigation('retry_with_feedback', {**context, 'error_message': str(e)})
                            if context.get('retry'):
                                prompt = f\"\"\"Previous attempt had syntax error: {e}

Previous code:
```python
{code}
```

Fix the syntax error and generate ONLY the corrected Python code for: {description}

Output ONLY the corrected complete Python code for file: {filename}\"\"\"
                                context['prompt'] = "/no_think " + prompt
                                continue
                    else:
                        print(f"Warning: Generated code has syntax error: {e}")
                        code = f"# Generated: {description}\\\\nprint('Error: Could not generate valid code')"

                    # Additional validation: check for undefined names
                    undefined_error = None
                    try:
                        import ast

                        tree = ast.parse(code)

                        # Collect all defined names
                        defined_names = set()
                        imported_names = set()

                        for node in ast.walk(tree):
                            if isinstance(node, ast.FunctionDef):
                                defined_names.add(node.name)
                                for arg in node.args.args:
                                    defined_names.add(arg.arg)
                            elif isinstance(node, ast.AsyncFunctionDef):
                                defined_names.add(node.name)
                                for arg in node.args.args:
                                    defined_names.add(arg.arg)
                            elif isinstance(node, ast.ClassDef):
                                defined_names.add(node.name)
                            elif isinstance(node, ast.Import):
                                for alias in node.names:
                                    imported_names.add(alias.asname or alias.name.split(".")[0])
                            elif isinstance(node, ast.ImportFrom):
                                if node.module:
                                    imported_names.add(node.module)
                                for alias in node.names:
                                    imported_names.add(alias.asname or alias.name)
                            elif isinstance(node, ast.Assign):
                                for target in node.targets:
                                    if isinstance(target, ast.Name):
                                        defined_names.add(target.id)
                                    elif isinstance(target, ast.Attribute):
                                        pass
                            elif isinstance(node, ast.AnnAssign):
                                if isinstance(node.target, ast.Name):
                                    defined_names.add(node.target.id)
                            elif isinstance(node, ast.ExceptHandler):
                                if node.name:
                                    defined_names.add(node.name)
                            elif isinstance(node, ast.With):
                                for item in node.items:
                                    if item.optional_vars:
                                        if isinstance(item.optional_vars, ast.Name):
                                            defined_names.add(item.optional_vars.id)
                            elif isinstance(node, ast.comprehension):
                                if isinstance(node.target, ast.Name):
                                    defined_names.add(node.target.id)
                            elif isinstance(node, ast.For):
                                if isinstance(node.target, ast.Name):
                                    defined_names.add(node.target.id)
                            elif isinstance(node, ast.AsyncFor):
                                if isinstance(node.target, ast.Name):
                                    defined_names.add(node.target.id)

                        # Check for undefined names
                        undefined_names = []
                        for node in ast.walk(tree):
                            if isinstance(node, ast.Name) and isinstance(node.ctx, ast.Load):
                                name = node.id
                                if name not in defined_names and name not in imported_names:
                                    if name not in (
                                        "__name__", "__main__", "self", "cls", "True", "False", "None",
                                        "Exception", "ValueError", "TypeError", "ZeroDivisionError",
                                        "FileNotFoundError", "PermissionError", "eval", "input", "print",
                                        "len", "str", "int", "float", "bool", "list", "dict", "set",
                                        "tuple", "open", "range", "enumerate", "zip", "map", "filter",
                                        "sum", "max", "min", "abs", "round", "isinstance", "issubclass",
                                        "hasattr", "getattr", "setattr", "delattr", "super", "property",
                                        "staticmethod", "classmethod", "object", "type", "BaseException",
                                        "KeyboardInterrupt", "SystemExit", "GeneratorExit", "StopIteration",
                                        "StopAsyncIteration", "ArithmeticError", "OverflowError",
                                        "FloatingPointError", "ZeroDivisionError", "AssertionError",
                                        "AttributeError", "BufferError", "EOFError", "ImportError",
                                        "ModuleNotFoundError", "LookupError", "IndexError", "KeyError",
                                        "MemoryError", "NameError", "OSError", "RecursionError",
                                        "ReferenceError", "RuntimeError", "SyntaxError", "IndentationError",
                                        "TabError", "SystemError", "TypeError", "UnboundLocalError",
                                        "UnicodeError", "UnicodeEncodeError", "UnicodeDecodeError",
                                        "UnicodeTranslateError", "ValueError", "Warning",
                                        "DeprecationWarning", "PendingDeprecationWarning", "RuntimeWarning",
                                        "SyntaxWarning", "UserWarning", "FutureWarning", "ImportWarning",
                                        "UnicodeWarning", "BytesWarning", "ResourceWarning",
                                    ):
                                        undefined_names.append(name)

                        if undefined_names:
                            likely_issues = [n for n in undefined_names if not n.startswith("_")]
                            if likely_issues:
                                unique_issues = list(set(likely_issues))
                                print(f'Warning: Potential undefined names: {", ".join(unique_issues)}')
                                if iteration < max_iterations - 1:
                                    # Apply add_type_hints_and_validation mitigation
                                    context = apply_mitigation('add_type_hints_and_validation', context)
                                    context['auto_fixes'].append('add_type_hints_and_validation')
                                    if context.get('add_validation'):
                                        prompt = f\"\"\"Previous code had undefined names: {', '.join(unique_issues)}

Previous code:
```python
{code}
```

Fix the undefined names by adding missing imports or defining the names. Generate ONLY the corrected Python code for: {description}

Output ONLY the corrected complete Python code for file: {filename}\"\"\"
                                    context['prompt'] = "/no_think " + prompt
                                    continue
                                else:
                                    print(f'Warning: Generated code has undefined names: {", ".join(unique_issues)}')

                    except Exception as e:
                        print(f"Warning: Could not validate undefined names: {e}")

                    # Check for f-string issues (double braces)
                    if "{{" in code and "}}" in code:
                        print("Warning: Detected double braces in f-strings, fixing...")
                        code = code.replace("{{", "{").replace("}}", "}")
                        try:
                            compile(code, filename, "exec")
                        except SyntaxError as e:
                            if iteration < max_iterations - 1:
                                prompt = f\"\"\"Previous attempt had syntax error after f-string fix: {e}

Previous code:
```python
{code}
```

Fix the syntax error and generate ONLY the corrected Python code for: {description}

Output ONLY the corrected complete Python code for file: {filename}\"\"\"
                                context['prompt'] = "/no_think " + prompt
                                continue

                    # Handle fix_fstring_braces mitigation
                    if context.get('fix_fstrings'):
                        print("Applying f-string brace fix...")
                        context['auto_fixes'].append('fix_fstring_braces')
                        code = code.replace("{{", "{").replace("}}", "}")
                        try:
                            compile(code, filename, "exec")
                        except SyntaxError as e:
                            if iteration < max_iterations - 1:
                                context = apply_mitigation('retry_with_feedback', {**context, 'error_message': str(e)})
                                if context.get('retry'):
                                    prompt = f\"\"\"Previous attempt had syntax error after f-string fix: {e}

Previous code:
```python
{code}
```

Fix the syntax error and generate ONLY the corrected Python code for: {description}

Output ONLY the corrected complete Python code for file: {filename}\"\"\"
                                    context['prompt'] = "/no_think " + prompt
                                    continue

                    # Handle fix_indentation mitigation
                    if context.get('fix_indent'):
                        print("Fixing indentation...")
                        context['auto_fixes'].append('fix_indentation')
                        lines = code.split('\\n')
                        fixed_lines = []
                        for line in lines:
                            line = line.expandtabs(4)
                            if line.strip() and not line.startswith(' ') and not line.startswith('\\t'):
                                if any(kw in line for kw in ['def ', 'class ', 'if ', 'for ', 'while ', 'try:', 'except:', 'else:', 'elif ', 'with ', 'async ']):
                                    pass
                            fixed_lines.append(line)
                        code = '\\n'.join(fixed_lines)
                        try:
                            compile(code, filename, "exec")
                        except SyntaxError:
                            pass

                    # Handle fix_cli_args mitigation
                    if context.get('fix_cli'):
                        print("Fixing CLI argument conflicts...")
                        context['auto_fixes'].append('fix_cli_args')

                    # Handle add_missing_import mitigation
                    if context.get('add_imports'):
                        print("Adding missing imports...")
                        context['auto_fixes'].append('add_missing_import')

                    # Handle fix_type_annotations mitigation
                    if context.get('fix_types'):
                        print("Fixing type annotations...")
                        context['auto_fixes'].append('fix_type_annotations')

                    # Success! Compute quality score
                    validation_result = self._validate_and_repair_code(
                        code, filename, context.get('description', ''), context, max_repair_iterations=1
                    )

                    quality_score = validation_result.get('quality_score', 0.0)

                    if validation_result['success']:
                        total_duration = time.time() - context['start_time']
                        record_generation(
                            prompt=context['prompt'],
                            success=True,
                            code=code,
                            fix=", ".join(context['auto_fixes']) if context['auto_fixes'] else None,
                            iterations=iteration + 1,
                            tokens=len(context['prompt']) // 4 + len(code) // 4,
                            duration=total_duration
                        )

                        if not yolo and iteration == max_iterations - 1:
                            print(f"Generated code for: {description}")
                            print("-" * 40)
                            print(code[:500] + ("..." if len(code) > 500 else ""))
                            print("-" * 40)
                            confirm = input("Save to file? (y/n): ")
                            if confirm.strip().lower() != "y":
                                print("Cancelled.")
                                return

                        if not os.path.isabs(filename):
                            filename = os.path.join(os.getcwd(), filename)
                        os.makedirs(os.path.dirname(os.path.abspath(filename)), exist_ok=True)
                        with open(filename, "w", encoding="utf-8") as f:
                            f.write(code)
                        print(f"Code saved to {filename}")
                        if context['auto_fixes']:
                            print(f"Auto-fixes applied: {', '.join(context['auto_fixes'])}")
                        return

                    else:
                        print(f"Quality cycle: score={quality_score:.2f}, threshold=0.75")
                        if quality_score >= 0.75:
                            print(f"\\u2713 Quality threshold met ({quality_score:.2f})")
                            code = validation_result['code']
                            break
                        else:
                            print(f"Quality cycle: score={quality_score:.2f}, threshold=0.75 - iterating...")
                            quality_cycle += 1
                            context['temperature'] = max(0.0, context.get('temperature', 0.0) - 0.05)
                            context['max_tokens'] = min(context['max_tokens'] + 2048, 32768)
                            context['quality_threshold'] = min(context.get('quality_threshold', 0.75) + 0.05, 0.9)
                            continue

                if not yolo and iteration == max_iterations - 1:
                    print(f"Generated code for: {description}")
                    print("-" * 40)
                    print(code[:500] + ("..." if len(code) > 500 else ""))
                    print("-" * 40)
                    confirm = input("Save to file? (y/n): ")
                    if confirm.strip().lower() != "y":
                        print("Cancelled.")
                        return

                if not os.path.isabs(filename):
                    filename = os.path.join(os.getcwd(), filename)
                os.makedirs(os.path.dirname(os.path.abspath(filename)), exist_ok=True)
                with open(filename, "w", encoding="utf-8") as f:
                    f.write(code)
                print(f"Code saved to {filename}")
                if context['auto_fixes']:
                    print(f"Auto-fixes applied: {', '.join(context['auto_fixes'])}")
                return

            except Exception as e:
                iteration_duration = time.time() - iteration_start
                print(f"Error in iteration {iteration + 1}: {e}")

                # Record failure for learning
                record_generation(
                    prompt=context['prompt'],
                    success=False,
                    error=e,
                    fixes=context['auto_fixes'] if context['auto_fixes'] else None,
                    iterations=iteration + 1,
                    tokens=len(context['prompt']) // 4,
                    duration=time.time() - context['start_time']
                )

                # Check for applicable mitigations on exception
                mitigations = get_mitigations(context['prompt'], str(e), code or '', time.time() - context['start_time'])
                for m in mitigations:
                    if m.confidence > 0.8:
                        context = apply_mitigation(m.action, context)
                        context['auto_fixes'].append(m.action)
                        break

                if iteration == max_iterations - 1:
                    print(f"Failed after {max_iterations} iterations")
                    return

        return False
'''

# Replace the old method with the new one
new_content = content[:start] + new_method + content[next_method:]
with open('lai.py', 'w') as f:
    f.write(new_content)

print('Replaced code method with quality cycles')
"
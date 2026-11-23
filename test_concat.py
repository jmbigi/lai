# Test the fix for lai.py string concatenation issue

# Test 1: Current problematic code
corpus_section = f"""
# CORPUS DE CONTEXTO (referencia)
test corpus
"""

base_prompt = 'test prompt'

try:
    prompt = ("/no_think " + 'base_prompt' + corpus_section + 
        """Task: {description}

Generate ONLY the complete Python code for file: {filename}

IMPORTANT: Use straight quotes (\') or (\"), NO curly quotes. Use pure ASCII. NO markdown, no explanations.\"\"\").format(description='test', filename='test.py')
    print("Test 1 FAILED - should have raised SyntaxError")
except SyntaxError as e:
    print(f"Test 1: Expected SyntaxError - {e}")

# Test 2: Fix by making corpus_section a regular string (not triple-quoted)
corpus_section = (
    "\n"
    "# CORPUS DE CONTEXTO (referencia)\n"
    "{corpus}\n"
    "\n"
).format(corpus="test corpus")

base_prompt = "test prompt"

try:
    prompt = ("/no_think " + "base_prompt" + corpus_section + 
        """Task: {description}

Generate ONLY the complete Python code for file: {filename}

IMPORTANT: Use straight quotes (\') or (\"), NO curly quotes. Use pure ASCII. NO markdown, no explanations.""").format(description='test', filename='test.py')
    print("Test 2: SUCCESS")
    print(repr(prompt[:100]))
except SyntaxError as e:
    print(f"Test 2 FAILED: {e}")
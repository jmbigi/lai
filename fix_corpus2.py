with open('lai.py', 'r') as f:
    lines = f.readlines()

# Print current state around the problematic area
print("Current lines 935-955:")
for i in range(935, 955):
    if i < len(lines):
        print(f'{i+1:4d}: {lines[i].rstrip()}')

# We need to replace lines 936-950 (0-indexed 935-950)
# The problematic section is from line 937 (index 936) to line 951 (index 950)

new_block = [
    '        corpus_section = (\\n',
    '            "\\n"\\n',
    '            "# CORPUS DE CONTEXTO (referencia)\\\\n"\\n',
    '            "{corpus}\\\\n"\\n',
    '            "\\n"\\n',
    '        ).format(corpus=corpus)\\n',
    '\\n',
    '        prompt = ("/no_think " + base_prompt + corpus_section +\\n',
    '    """Task: {description}\\n',
    'Generate ONLY the complete Python code for file: {filename}\\n',
    '\\n',
    'IMPORTANT: Use straight quotes (\\') or (\\"), NO curly quotes. Use pure ASCII. NO markdown, no explanations.\"\"\").format(description=description, filename=filename)\\n',
]

# Replace lines 936-950 (0-indexed: 936-950)
# lines[936:951] = new lines
new_lines = lines[:936] + [l + '\\n' for l in [
    '        corpus_section = (',
    '            "\\n"',
    '            "# CORPUS DE CONTEXTO (referencia)\\\\n"',
    '            "{corpus}\\\\n"',
    '            "\\n"',
    '        ).format(corpus=corpus)',
    '',
    '        prompt = ("/no_think " + base_prompt + corpus_section +',
    '    """Task: {description}',
    '',
    'Generate ONLY the complete Python code for file: {filename}',
    '',
    'IMPORTANT: Use straight quotes (\') or (\"), NO curly quotes. Use pure ASCII. NO markdown, no explanations.\"\"\").format(description=description, filename=filename)',
] + lines[951:]

with open('lai.py', 'w') as f:
    f.writelines(new_lines)

print('Fixed')
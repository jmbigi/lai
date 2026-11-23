with open('lai.py', 'r') as f:
    lines = f.readlines()

# Fix lines 937-944 (indices 936-943)
# The corpus_section definition is broken

new_lines = [
    '        corpus_section = (\n',
    '            "\\n"\n',
    '            "# CORPUS DE CONTEXTO (referencia)\\\\n"\n',
    '            "{corpus}\\\\n"\n',
    '            "\\n"\n',
    '        ).format(corpus=corpus)\n',
    '\n',
]

# Replace lines 937-944 (indices 936-943)
lines[936:944] = [l + '\n' for l in [
    '        corpus_section = (',
    '            "\\n"',
    '            "# CORPUS DE CONTEXTO (referencia)\\\\n"',
    '            "{corpus}\\\\n"',
    '            "\\n"',
    '        ).format(corpus=corpus)',
    '',
]]

with open('lai.py', 'w') as f:
    f.writelines(lines)

print('Fixed corpus_section')
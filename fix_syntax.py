with open('lai.py', 'r') as f:
    content = f.read()

# Find the problematic string concatenation
old = '''prompt = ("/no_think " + base_prompt + corpus_section + 
    """Task: {description}

Generate ONLY the complete Python code for file: {filename}

IMPORTANT: Use straight quotes (\') or (\"), NO curly quotes. Use pure ASCII. NO markdown, no explanations.""").format(description=description, filename=filename)'''

new = '''prompt = ("/no_think " + base_prompt + corpus_section + 
    + """Task: {description}

Generate ONLY the complete Python code for file: {filename}

IMPORTANT: Use straight quotes (\') or (\"), NO curly quotes. Use pure ASCII. NO markdown, no explanations.""").format(description=description, filename=filename)'''

if old in content:
    content = content.replace(old, new)
    with open('lai.py', 'w') as f:
        f.write(content)
    print('Fixed')
else:
    print('Pattern not found')
    # Try to find the exact pattern
    idx = content.find('prompt = ("/no_think " + base_prompt + corpus_section + ')
    if idx >= 0:
        print('Found at index:', idx)
        print(repr(content[idx:idx+200]))
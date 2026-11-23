with open('lai.py', 'r') as f:
    lines = f.readlines()

# Show current state
print("Current lines 935-955:")
for i in range(935, 955):
    if i < len(lines):
        print(f'{i+1:4d}: {lines[i].rstrip()}')

# We'll do a simple string replacement on the whole file content
with open('lai.py', 'r') as f:
    content = f.read()

# The problematic section is the corpus_section definition and prompt concatenation
# We need to replace the broken multi-line string with a proper one

# Find the problematic section
start_marker = 'corpus_section = """'
end_marker = '        prompt = ("/no_think " + base_prompt + corpus_section +'

start_idx = content.find(start_marker)
end_idx = content.find(end_marker)

if start_marker in content and end_marker in content:
    print("Found markers")
    # Extract the problematic section
    before = content[:content.find(start_marker)]
    middle_start = content.find(start_marker)
    middle_end = content.find(end_marker)
    
    # The new middle section
    new_middle = '''corpus_section = (
            "\\n"
            "# CORPUS DE CONTEXTO (referencia)\\\\n"
            "{corpus}\\\\n"
            "\\n"
        ).format(corpus=corpus)

        prompt = ("/no_think " + base_prompt + corpus_section +
    """Task: {description}

Generate ONLY the complete Python code for file: {filename}

IMPORTANT: Use straight quotes (\') or (\"), NO curly quotes. Use pure ASCII. NO markdown, no explanations.""").format(description=description, filename=filename)'''
    
    # Find the exact positions
    start_idx = content.find('corpus_section = """')
    end_idx = content.find('        prompt = ("/no_think " + base_prompt + corpus_section +')
    
    if start_marker in content and end_marker in content:
        # Get the parts
        before = content[:content.find(start_marker)]
        after = content[content.find(end_marker):]
        
        new_content = before + new_middle + after
        
        with open('lai.py', 'w') as f:
            f.write(new_content)
        print('Fixed')
    else:
        print('Markers not found')
"
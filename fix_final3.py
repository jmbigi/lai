import re

with open('lai.py', 'r') as f:
    content = f.read()

# Fix the pattern: + \n    + '''Task: -> \n    + """Task:
# The pattern is: +\n\s+\+\s*'''Task:
content = re.sub(r'\+\n\s+\+\s*\'{3}Task:', '\n    + """Task:', content)

# Also fix any remaining f-string issues with triple double quotes
content = content.replace('f"""', "f'''")
content = content.replace('"""', "'''")

with open('lai.py', 'w') as f:
    f.write(content)

print('Fixed')
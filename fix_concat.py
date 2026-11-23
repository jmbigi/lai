with open('lai.py', 'r') as f:
    content = f.read()

# Use regex to fix the string concatenation issue
import re

# Pattern: find "+ \n    + '''Task:" and replace with just newline + triple quotes
# The pattern is: +\n    + '''Task:
content = re.sub(
    r'\+\n\s+\+\s*\'{3}Task:',
    '\n    """Task:',
    content
)

# Also fix any remaining f-string issues with triple double quotes
content = content.replace('f"""', "f'''")
content = content.replace('"""', "'''")

with open('lai.py', 'w') as f:
    f.write(content)

print('Fixed')
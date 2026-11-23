with open('lai.py', 'r') as f:
    lines = f.readlines()

# Fix lines 1255-1265 (indices 1254-1265)
# The structure should be:
#                         if context['auto_fixes']:
#                     fixes = ", ".join(context["auto_fixes"])
#                     print(f"Auto-fixes applied: {fixes}")
#                     return
#                         # Quality cycle logic
#                         if quality_score >= 0.75:
#                             print(f"✓ Quality threshold met ({quality_score:.2f})")
#                             code = validation_result['code']
#                             break
#                         else:
#                             print(f"Quality cycle: score={quality_score:.2f}, threshold=0.75 - iterating...")
#                             quality_cycle += 1
#                             context['temperature'] = max(0.0, context.get('temperature', 0.0) - 0.05)
#                             context['max_tokens'] = min(context.get('max_tokens', 8192) + 2048, 32768)
#                             context['quality_threshold'] = min(context.get('quality_threshold', 0.75) + 0.05, 0.9)
#                             continue

new_block = [
    '                        if context["auto_fixes"]:\n',
    '                    fixes = ", ".join(context["auto_fixes"])\n',
    '                    print(f"Auto-fixes applied: {fixes}")\n',
    '                    return\n',
    '                        # Quality cycle logic\n',
    '                        if quality_score >= 0.75:\n',
    '                            print(f"\u2713 Quality threshold met ({quality_score:.2f})")\n',
    '                            code = validation_result["code"]\n',
    '                            break\n',
    '                        else:\n',
    '                            print(f"Quality cycle: score={quality_score:.2f}, threshold=0.75 - iterating...")\n',
    '                            quality_cycle += 1\n',
    '                            context["temperature"] = max(0.0, context.get("temperature", 0.0) - 0.05)\n',
    '                            context["max_tokens"] = min(context.get("max_tokens", 8192) + 2048, 32768)\n',
    '                            context["quality_threshold"] = min(context.get("quality_threshold", 0.75) + 0.05, 0.9)\n',
    '                            continue\n',
]

# Replace lines 1255-1275 (indices 1254-1274)
# The block starts at line 1255 (index 1254) with 'if context["auto_fixes"]:'
# and ends before the next major block

# First, let's find the exact lines
with open('lai.py', 'r') as f:
    lines = f.readlines()

# Find the start of the block
start_idx = None
for i, line in enumerate(lines):
    if 'if context["auto_fixes"]:' in line and i > 1250:
        start_idx = i
        break

# Find the end of the block (next major block)
end_idx = None
for i in range(start_idx, len(lines)):
    if 'if not yolo and iteration == max_iterations - 1:' in lines[i]:
        end_idx = i
        break

if 'start_idx' in locals() and 'end_idx' in locals():
    print(f'Found block from line {start_idx+1} to {end_idx+1}')
    # Replace
    new_lines = lines[:start_idx] + [l + '\n' for l in [
        '                        if context["auto_fixes"]:\n',
        '                    fixes = ", ".join(context["auto_fixes"])\n',
        '                    print(f"Auto-fixes applied: {fixes}")\n',
        '                    return\n',
        '                        # Quality cycle logic\n',
        '                        if quality_score >= 0.75:\n',
        '                            print(f"\u2713 Quality threshold met ({quality_score:.2f}")\n',
        '                            code = validation_result["code"]\n',
        '                            break\n',
    '                        else:\n',
    '                            print(f"Quality cycle: score={quality_score:.2f}, threshold=0.75 - iterating...")\n',
    '                            quality_cycle += 1\n',
    '                            context["temperature"] = max(0.0, context.get("temperature", 0.0) - 0.05)\n',
    '                            context["max_tokens"] = min(context.get("max_tokens", 8192) + 2048, 32768)\n',
    '                            context["quality_threshold"] = min(context.get("quality_threshold", 0.75) + 0.05, 0.9)\n',
    '                            continue\n',
    ] + lines[end_idx:]

    with open('lai.py', 'w') as f:
        f.writelines(new_lines)

    print('Fixed')
else:
    print('Could not find block')
" 2>&1
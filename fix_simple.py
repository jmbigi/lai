with open('lai.py', 'r') as f:
    lines = f.readlines()

# Fix line 1256 (index 1255): if context['auto_fixes']: - should be 28 spaces
lines[1255] = '                        if context["auto_fixes"]:\n'

# Fix line 1257 (index 1256): fixes = ... should be 32 spaces (8 tabs)
lines[1256] = '                    fixes = ", ".join(context["auto_fixes"])\n'

# Fix line 1258 (index 1257): print statement - 32 spaces
lines[1257] = '                    print(f"Auto-fixes applied: {fixes}")\n'

# Fix line 1258 (index 1257): return - 28 spaces
lines[1257] = '                        return\n'

# Line 1258 (index 1257) is duplicate return - remove
lines[1257] = ''

# Line 1258 (index 1257) is blank - keep
# Line 1259 (idx 1257) is blank - keep

# Line 1259 (idx 1258) is 'if quality_score >= 0.75:' - should be 28 spaces
# But it's currently at 24 spaces. Fix indentation.
lines[1258] = '                        if quality_score >= 0.75:\n'

# Line 1259 (idx 1258): print - should be 28 spaces
lines[1258] = '                            print(f"\u2713 Quality threshold met ({quality_score:.2f}")\n'

# Line 1259 (idx 1258): code = ... - 28 spaces
lines[1259] = '                            code = validation_result["code"]\n'

# Line 1259 (idx 1258): break - 28 spaces
lines[1259] = '                            break\n'

# Line 1260 (idx 1259): else: - should be 28 spaces
lines[1260] = '                        else:\n'

# Line 1261 (idx 1260): print - 28 spaces
lines[1261] = '                            print(f"Quality cycle: score={quality_score:.2f}, threshold=0.75 - iterating...")\n'

# Line 1262 (idx 1261): quality_cycle += 1 - 28 spaces
lines[1262] = '                            quality_cycle += 1\n'

# Line 1263 (idx 1262): context['temperature'] = ... - 28 spaces
lines[1263] = '                            context["temperature"] = max(0.0, context.get("temperature", 0.0) - 0.05)\n'

# Line 1264 (idx 1263): context['max_tokens'] = ...
lines[1264] = '                            context["max_tokens"] = min(context.get("max_tokens", 8192) + 2048, 32768)\n'

# Line 1265 (idx 1264): context['quality_threshold'] = ...
lines[1265] = '                            context["quality_threshold"] = min(context.get("quality_threshold", 0.75) + 0.05, 0.9)\n'

# Line 1266 (idx 1266): continue
lines[1266] = '                            continue\n'

with open('lai.py', 'w') as f:
    f.writelines(lines)

print('Fixed indentation')
# This is a complete rewrite of the _run_quality_cycle method and related methods
# to replace the broken section in lai.py

def _run_quality_cycle(self, description, filename, context, max_iterations, yolo):
    """
    Run a single quality cycle with full validation and repair.
    Returns dict with success, code, quality_score, repair_attempted, fixes_applied
    """
    context = context.copy()
    context['iteration'] = 0
    context['auto_fixes'] = []
    
    max_repair_iterations = 2
    best_code = None
    best_quality = 0.0
    
    for repair_iter in range(max_repair_iterations + 1):
        # Generate code
        if repair_iter > 0:
            # Apply stricter settings for repair attempts
            context['max_tokens'] = min(context['max_tokens'] + 2048, 32768)
            context['temperature'] = max(0.0, context.get('temperature', 0.0) - 0.1)

        # Generate code
        response = self.chat(
            context['prompt'], 
            stream=False, 
            thinking=False, 
            temp=context.get('temperature', 0.0), 
            max_tokens=context.get('max_tokens', 8192)
        )
        code = response.strip()
        
        # Extract code
        code = self._extract_code_from_response(code)
        
        # Apply strict extraction if needed
        if context.get('strict_extraction'):
            code = self._strict_code_extraction(code)
        
        # Run validation and compute quality score
        validation_result = self._validate_and_repair_code(
            code, filename, context.get('description', ''), context, max_repair_iterations=1
        )
        
        quality_score = validation_result.get('quality_score', 0.0)
        
        if validation_result['success']:
            return {
                'success': True,
                'code': validation_result['code'],
                'quality_score': quality_score,
                'repair_attempted': repair_iter > 0,
                'fixes_applied': validation_result.get('fixes_applied', [])
            }
        
        # If validation failed but we can repair
        if repair_iter < max_repair_iterations and validation_result.get('repair_attempted'):
            context['prompt'] = f"/no_think Fix ALL issues in this Python code. The code must pass ALL validations.\n\n```python\n{code}\n```\n\nVALIDATION ERRORS TO FIX:\n" + "\n".join(validation_result['errors']) + f"\n\nSTRICT RULES - NO EXCEPTIONS:\n- Fix EVERY error listed above\n- Generate COMPLETE working code, no stubs\n- NO input() calls - use argparse/sys.argv\n- NO bare except, NO pass in functions\n- NO NotImplementedError, NO pass as body\n- Type hints for ALL params and returns\n- Real implementations only\n\nGenerate ONLY the corrected complete Python code for file: {context['filename']}"
            continue
        
        # Track best attempt
        if quality_score > best_quality:
            best_quality = validation_result.get('quality_score', 0.0)
            best_code = validation_result.get('code', '')
    
    # Return best attempt even if not perfect
    return {
        'success': best_quality > 0.5,
        'code': best_code or '',
        'quality_score': best_quality,
        'repair_attempted': True,
        'fixes_applied': ['best_effort']
    }
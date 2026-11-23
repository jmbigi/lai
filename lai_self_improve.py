#!/usr/bin/env python3
"""
lai Self-Improvement Module - Advanced Version
Automatically learns from generation failures, applies mitigations, and optimizes prompts.
"""

import json
import os
import re
import time
import ast
import statistics
from dataclasses import dataclass, field, asdict
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple, Set
from collections import defaultdict
import hashlib


@dataclass
class GenerationRecord:
    """Record of a code generation attempt."""
    timestamp: str
    prompt_hash: str
    prompt: str
    success: bool
    error_type: Optional[str]
    error_message: Optional[str]
    generated_code: Optional[str]
    fixes_applied: List[str] = field(default_factory=list)
    iterations: int = 1
    tokens_used: int = 0
    duration_seconds: float = 0.0
    code_quality_score: float = 0.0
    prompt_category: str = "unknown"
    model_response_preview: str = ""


@dataclass
class MitigationRule:
    """A mitigation rule learned from failures."""
    pattern: str
    condition: str
    action: str
    confidence: float
    times_applied: int = 0
    times_successful: int = 0
    created_at: str = field(default_factory=lambda: datetime.now().isoformat())
    last_used: Optional[str] = None
    metadata: Dict = field(default_factory=dict)


@dataclass
class PromptPattern:
    """Learned successful prompt pattern."""
    category: str
    template: str
    success_count: int = 0
    total_uses: int = 0
    avg_tokens: int = 0
    avg_duration: float = 0.0
    keywords: List[str] = field(default_factory=list)


class SelfImprovingEngine:
    """Advanced engine that learns from failures, applies mitigations, and optimizes prompts."""
    
    def __init__(self, storage_path: str = None):
        if storage_path is None:
            storage_path = os.path.join(
                os.path.dirname(os.path.abspath(__file__)), 
                "self_improve_data.json"
            )
        self.storage_path = storage_path
        self.records: List[GenerationRecord] = []
        self.mitigation_rules: List[MitigationRule] = []
        self.prompt_patterns: Dict[str, PromptPattern] = {}
        self.error_signatures: Dict[str, int] = defaultdict(int)
        self.success_signatures: Dict[str, int] = defaultdict(int)
        self.fix_effectiveness: Dict[str, Dict[str, int]] = defaultdict(lambda: {"success": 0, "total": 0})
        self.load()
        
        # Built-in mitigation rules (pre-seeded from experience)
        self._init_builtin_rules()
    
    def _init_builtin_rules(self):
        """Initialize with rules learned from our experience."""
        builtin_rules = [
            MitigationRule(
                pattern="timeout",
                condition="duration_seconds > 100 or 'timeout' in error_message.lower()",
                action="split_prompt",
                confidence=0.9,
                times_applied=5,
                times_successful=4,
                metadata={"description": "Split complex prompt into subtasks"}
            ),
            MitigationRule(
                pattern="truncation",
                condition="'length' in error_message.lower() or 'truncat' in error_message.lower() or len(generated_code or '') > 6000",
                action="increase_max_tokens",
                confidence=0.95,
                times_applied=8,
                times_successful=7,
                metadata={"description": "Double max_tokens up to 32K"}
            ),
            MitigationRule(
                pattern="reasoning_leak",
                condition="any(w in (generated_code or '').lower() for w in ['okay,', 'here is', 'here\\'s', 'certainly', 'alright,', 'the code', 'the function', 'i will', 'let me', 'here\\'s the', 'below is', 'as requested', 'per your'])",
                action="add_no_think_prefix",
                confidence=0.9,
                times_applied=10,
                times_successful=9,
                metadata={"description": "Prefix prompt with /no_think"}
            ),
            MitigationRule(
                pattern="syntax_error",
                condition="'syntax' in error_message.lower() or 'invalid syntax' in error_message.lower()",
                action="retry_with_feedback",
                confidence=0.85,
                times_applied=15,
                times_successful=12,
                metadata={"description": "Retry with specific error feedback"}
            ),
            MitigationRule(
                pattern="undefined_names",
                condition="'undefined' in error_message.lower() or 'potential undefined' in error_message.lower()",
                action="add_type_hints_and_validation",
                confidence=0.8,
                times_applied=10,
                times_successful=7,
                metadata={"description": "Add missing imports/definitions"}
            ),
            MitigationRule(
                pattern="conversational_output",
                condition="any(w in (generated_code or '').lower() for w in ['explanation:', 'example:', 'usage:', 'note:', 'this function', 'this class', 'the following', 'here is an', 'below is an', 'here is the'])",
                action="strengthen_extraction",
                confidence=0.85,
                times_applied=8,
                times_successful=6,
                metadata={"description": "Use stricter code extraction"}
            ),
            MitigationRule(
                pattern="complex_prompt",
                condition="len(prompt) > 1500 or prompt.count(',') > 8 or len(re.findall(r'\\d+\\.', prompt)) > 3",
                action="decompose_prompt",
                confidence=0.8,
                times_applied=6,
                times_successful=4,
                metadata={"description": "Break into sequential steps"}
            ),
            MitigationRule(
                pattern="config_env_merge_bug",
                condition="'config' in prompt.lower() and 'env' in prompt.lower() and ('merge' in prompt.lower() or 'environment' in prompt.lower())",
                action="add_env_prefix_filter",
                confidence=0.9,
                times_applied=2,
                times_successful=2,
                metadata={"description": "Filter env vars with APP_ prefix"}
            ),
            MitigationRule(
                pattern="entropy_calculation_bug",
                condition="'entropy' in prompt.lower() and 'password' in prompt.lower()",
                action="fix_entropy_formula",
                confidence=0.95,
                times_applied=1,
                times_successful=1,
                metadata={"description": "Use correct entropy formula: length * log2(charset)"}
            ),
            MitigationRule(
                pattern="cli_argument_conflict",
                condition="'argparse' in error_message.lower() and ('conflict' in error_message.lower() or 'duplicate' in error_message.lower())",
                action="fix_cli_args",
                confidence=0.95,
                times_applied=1,
                times_successful=1,
                metadata={"description": "Use unique short flags"}
            ),
            MitigationRule(
                pattern="f_string_brace_error",
                condition="'f.string' in error_message.lower() and 'brace' in error_message.lower() or 'format' in error_message.lower() and 'brace' in error_message.lower()",
                action="fix_fstring_braces",
                confidence=0.9,
                times_applied=3,
                times_successful=3,
                metadata={"description": "Escape double braces in f-strings"}
            ),
            MitigationRule(
                pattern="indentation_error",
                condition="'indentation' in error_message.lower() or 'indent' in error_message.lower() or 'unexpected indent' in error_message.lower()",
                action="fix_indentation",
                confidence=0.85,
                times_applied=4,
                times_successful=3,
                metadata={"description": "Normalize indentation to 4 spaces"}
            ),
            MitigationRule(
                pattern="import_error",
                condition="'no module named' in error_message.lower() or 'modulenotfounderror' in error_message.lower()",
                action="add_missing_import",
                confidence=0.85,
                times_applied=5,
                times_successful=4,
                metadata={"description": "Add missing import statement"}
            ),
            MitigationRule(
                pattern="type_annotation_error",
                condition="'type' in error_message.lower() and ('annotation' in error_message.lower() or 'hint' in error_message.lower() or 'generics' in error_message.lower())",
                action="fix_type_annotations",
                confidence=0.8,
                times_applied=3,
                times_successful=2,
                metadata={"description": "Fix type hint syntax"}
            ),
        ]
        
        for rule in builtin_rules:
            if not any(r.pattern == rule.pattern for r in self.mitigation_rules):
                self.mitigation_rules.append(rule)
    
    def load(self):
        """Load records, rules, and patterns from storage."""
        if os.path.exists(self.storage_path):
            try:
                with open(self.storage_path, 'r') as f:
                    data = json.load(f)
                
                for r in data.get('records', []):
                    self.records.append(GenerationRecord(**r))
                
                for r in data.get('rules', []):
                    self.mitigation_rules.append(MitigationRule(**r))
                
                for cat, p in data.get('patterns', {}).items():
                    self.prompt_patterns[cat] = PromptPattern(**p)
                
                self.error_signatures = defaultdict(int, data.get('error_signatures', {}))
                self.success_signatures = defaultdict(int, data.get('success_signatures', {}))
                fix_eff = data.get('fix_effectiveness', {})
                for k, v in fix_eff.items():
                    self.fix_effectiveness[k] = v
            except Exception as e:
                print(f"Warning: Could not load self-improve data: {e}")
    
    def save(self):
        """Save records, rules, and patterns to storage."""
        try:
            data = {
                'records': [asdict(r) for r in self.records[-500:]],  # Keep last 500
                'rules': [asdict(r) for r in self.mitigation_rules],
                'patterns': {k: asdict(v) for k, v in self.prompt_patterns.items()},
                'error_signatures': dict(self.error_signatures),
                'success_signatures': dict(self.success_signatures),
                'fix_effectiveness': dict(self.fix_effectiveness),
                'last_updated': datetime.now().isoformat()
            }
            with open(self.storage_path, 'w') as f:
                json.dump(data, f, indent=2)
        except Exception as e:
            print(f"Warning: Could not save self-improve data: {e}")
    
    def categorize_prompt(self, prompt: str) -> str:
        """Categorize prompt by type."""
        p = prompt.lower()
        if any(k in p for k in ['class', 'dataclass', 'struct', 'model', 'entity']):
            return 'class_definition'
        if any(k in p for k in ['decorator', '@', 'wrapper', 'contextmanager']):
            return 'decorator'
        if any(k in p for k in ['async', 'await', 'concurrent', 'parallel', 'threading', 'multiprocessing']):
            return 'async_code'
        if any(k in p for k in ['cli', 'click', 'argparse', 'command', 'subcommand']):
            return 'cli_tool'
        if any(k in p for k in ['config', 'yaml', 'env', 'settings', 'configuration']):
            return 'config_loader'
        if any(k in p for k in ['test', 'pytest', 'unittest', 'mock', 'fixture']):
            return 'testing'
        if any(k in p for k in ['api', 'http', 'request', 'client', 'server', 'endpoint', 'rest']):
            return 'api_client'
        if any(k in p for k in ['password', 'hash', 'crypto', 'security', 'entropy', 'encrypt', 'jwt', 'token']):
            return 'security'
        if any(k in p for k in ['parse', 'csv', 'json', 'xml', 'transform', 'serialize', 'deserialize']):
            return 'data_processing'
        if any(k in p for k in ['database', 'sql', 'sqlite', 'orm', 'query', 'migration']):
            return 'database'
        if any(k in p for k in ['cache', 'redis', 'memo', 'lru']):
            return 'caching'
        if any(k in p for k in ['log', 'logging', 'trace', 'monitor', 'metric']):
            return 'observability'
        return 'general'
    
    def extract_code_signature(self, code: str) -> str:
        """Extract a signature from code for pattern matching."""
        if not code:
            return ""
        try:
            tree = ast.parse(code)
            sig_parts = []
            for node in ast.walk(tree):
                if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    args = [a.arg for a in node.args.args]
                    sig_parts.append(f"{node.name}({','.join(args)})")
                elif isinstance(node, ast.ClassDef):
                    sig_parts.append(f"class {node.name}")
            return "|".join(sorted(sig_parts)[:5])
        except Exception:
            return ""
    
    def assess_code_quality(self, code: str) -> float:
        """Assess code quality (0-1 score)."""
        if not code:
            return 0.0
        score = 0.0
        try:
            tree = ast.parse(code)
            score += 0.15  # Basic syntax
            
            # Has type hints
            has_hints = any(
                isinstance(node, ast.FunctionDef) and (node.returns or any(a.annotation for a in node.args.args))
                for node in ast.walk(tree)
            )
            if has_hints:
                score += 0.2
            
            # Has docstrings
            has_docs = any(
                ast.get_docstring(node) for node in ast.walk(tree)
                if isinstance(node, (ast.FunctionDef, ast.ClassDef, ast.AsyncFunctionDef))
            )
            if has_docs:
                score += 0.2
            
            # Has main guard
            has_main = any(
                isinstance(node, ast.If) and 
                isinstance(node.test, ast.Compare) and
                any(isinstance(c, ast.Name) and c.id == '__name__' for c in node.test.comparators)
                for node in ast.walk(tree)
            )
            if has_main:
                score += 0.15
            
            # Has error handling
            has_try = any(isinstance(node, ast.Try) for node in ast.walk(tree))
            if has_try:
                score += 0.15
            
            # Has imports
            has_imports = any(isinstance(node, (ast.Import, ast.ImportFrom)) for node in ast.walk(tree))
            if has_imports:
                score += 0.1
                
        except Exception:
            pass
        return min(score, 1.0)
    
    def record_generation(self, record: GenerationRecord):
        """Record a generation attempt and learn from it."""
        record.prompt_category = self.categorize_prompt(record.prompt)
        record.code_quality_score = self.assess_code_quality(record.generated_code or "")
        record.model_response_preview = (record.generated_code or "")[:200]
        
        self.records.append(record)
        
        # Update signatures
        if record.success:
            sig = self.extract_code_signature(record.generated_code or "")
            if sig:
                self.success_signatures[sig] += 1
            
            # Learn successful prompt pattern
            cat = record.prompt_category
            if cat not in self.prompt_patterns:
                self.prompt_patterns[cat] = PromptPattern(
                    category=cat,
                    template=record.prompt[:200],
                    keywords=self._extract_keywords(record.prompt)
                )
            pattern = self.prompt_patterns[cat]
            pattern.success_count += 1
            pattern.total_uses += 1
            pattern.avg_tokens = (pattern.avg_tokens * (pattern.total_uses - 1) + record.tokens_used) / pattern.total_uses
            pattern.avg_duration = (pattern.avg_duration * (pattern.total_uses - 1) + record.duration_seconds) / pattern.total_uses
            
            # Track fix effectiveness
            for fix in record.fixes_applied:
                self.fix_effectiveness[fix]["success"] += 1
                self.fix_effectiveness[fix]["total"] += 1
        else:
            err_sig = record.error_type or "unknown"
            self.error_signatures[err_sig] += 1
            
            # Track failed fixes
            for fix in record.fixes_applied:
                self.fix_effectiveness[fix]["total"] += 1
        
        # Learn from failures
        if not record.success:
            self._learn_from_failure(record)
        
        # Keep only last 500 records
        if len(self.records) > 500:
            self.records = self.records[-500:]
        
        self.save()
    
    def _extract_keywords(self, prompt: str) -> List[str]:
        """Extract technical keywords from prompt."""
        keywords = []
        tech_terms = ['async', 'decorator', 'class', 'cli', 'config', 'yaml', 'json', 
                      'api', 'http', 'database', 'cache', 'thread', 'async', 'await',
                      'generator', 'context', 'manager', 'decorator', 'iterator',
                      'dataclass', 'pydantic', 'sqlalchemy', 'redis', 'sqlite',
                      'pytest', 'unittest', 'mock', 'fixture', 'parametrize']
        p = prompt.lower()
        for term in tech_terms:
            if term in p:
                keywords.append(term)
        return keywords
    
    def _learn_from_failure(self, record: GenerationRecord):
        """Analyze failure and create/update mitigation rules."""
        error_msg = (record.error_message or '').lower()
        code = record.generated_code or ''
        prompt = record.prompt or ''
        
        # Check existing rules
        for rule in self.mitigation_rules:
            try:
                local_vars = {
                    'error_message': error_msg,
                    'generated_code': code,
                    'prompt': prompt,
                    'duration_seconds': record.duration_seconds,
                    'iterations': record.iterations,
                    'tokens_used': record.tokens_used,
                }
                if eval(rule.condition, {"__builtins__": {}}, local_vars):
                    rule.times_applied += 1
                    rule.last_used = datetime.now().isoformat()
                    if record.fixes_applied and any(rule.action in f for f in record.fixes_applied):
                        rule.times_successful += 1
                    break
            except Exception:
                pass
        
        # Auto-create new rule if pattern is novel
        self._auto_create_rule(record)
    
    def _auto_create_rule(self, record: GenerationRecord):
        """Automatically create a mitigation rule from a failure pattern."""
        error_msg = (record.error_message or '').lower()
        code = record.generated_code or ''
        
        patterns = {
            'timeout': ('timeout|time out', "duration_seconds > 100"),
            'truncation': ('truncat|length|max_token', "len(generated_code or '') > 6000"),
            'reasoning': ('okay|here is|certainly|alright|the code|the function|i will|let me|here is the|below is|as requested|per your', 
                         "any(w in (generated_code or '').lower() for w in ['okay,', 'here is', 'here\\'s', 'certainly'])"),
            'syntax': ('syntax|invalid syntax', "'syntax' in error_message.lower()"),
            'undefined': ('undefined|potential undefined', "'undefined' in error_message.lower()"),
            'conversational': ('explanation|example|usage|note:', 
                              "any(w in (generated_code or '').lower() for w in ['explanation:', 'example:', 'usage:'])"),
            'cli_conflict': ('argparse.*conflict|duplicate.*option', 
                            "'argparse' in error_message.lower() and 'conflict' in error_message.lower()"),
            'fstring': ('f.string.*brace|format.*brace', 
                       "'f.string' in error_message.lower() or 'format' in error_message.lower()"),
            'indentation': ('indentation|indent|unexpected indent', 
                           "'indentation' in error_message.lower() or 'indent' in error_message.lower()"),
            'import': ('no module named|modulenotfound', 
                      "'no module named' in error_message.lower()"),
            'type_hint': ('type.*annotation|type.*hint|generics', 
                         "'type' in error_message.lower() and ('annotation' in error_message.lower() or 'hint' in error_message.lower())"),
        }
        
        for pattern_name, (keywords, condition) in patterns.items():
            if any(kw in error_msg or kw in code.lower() for kw in keywords.split('|')):
                if not any(r.pattern == pattern_name for r in self.mitigation_rules):
                    action_map = {
                        'timeout': 'split_prompt',
                        'truncation': 'increase_max_tokens',
                        'reasoning': 'add_no_think_prefix',
                        'syntax': 'retry_with_feedback',
                        'undefined': 'add_type_hints_and_validation',
                        'conversational': 'strengthen_extraction',
                        'cli_conflict': 'fix_cli_args',
                        'fstring': 'fix_fstring_braces',
                        'indentation': 'fix_indentation',
                        'import': 'add_missing_import',
                        'type_hint': 'fix_type_annotations',
                    }
                    new_rule = MitigationRule(
                        pattern=pattern_name,
                        condition=condition,
                        action=action_map.get(pattern_name, 'retry_with_feedback'),
                        confidence=0.7,
                        times_applied=1,
                        times_successful=0,
                        metadata={"auto_created": True, "source_error": record.error_message[:100]}
                    )
                    self.mitigation_rules.append(new_rule)
                    break
    
    def get_applicable_mitigations(self, prompt: str, error_message: str = '', 
                                   generated_code: str = '', duration: float = 0,
                                   tokens: int = 0, iteration: int = 1) -> List[MitigationRule]:
        """Get mitigation rules applicable to current situation, ranked by effectiveness."""
        applicable = []
        local_vars = {
            'error_message': error_message.lower(),
            'generated_code': generated_code or '',
            'prompt': prompt,
            'duration_seconds': duration,
            'iterations': iteration,
            'tokens_used': tokens,
        }
        
        for rule in self.mitigation_rules:
            try:
                if eval(rule.condition, {"__builtins__": {}}, local_vars):
                    # Calculate effectiveness score
                    success_rate = rule.times_successful / max(rule.times_applied, 1)
                    effectiveness = rule.confidence * success_rate * (1 + 0.1 * min(rule.times_applied, 10))
                    applicable.append((effectiveness, rule))
            except Exception:
                pass
        
        # Sort by effectiveness
        applicable.sort(key=lambda x: x[0], reverse=True)
        return [r for _, r in applicable]
    
    def apply_mitigation(self, action: str, context: Dict) -> Dict:
        """Apply a mitigation action and return modified context."""
        result = context.copy()
        
        if action == 'split_prompt':
            result = self._split_prompt(result)
        elif action == 'increase_max_tokens':
            result['max_tokens'] = min(result.get('max_tokens', 4096) * 2, 32768)
        elif action == 'add_no_think_prefix':
            if not result.get('prompt', '').startswith('/no_think'):
                result['prompt'] = '/no_think ' + result.get('prompt', '')
        elif action == 'retry_with_feedback':
            result['retry'] = True
            result['feedback'] = result.get('error_message', '')
        elif action == 'add_type_hints_and_validation':
            result['add_validation'] = True
        elif action == 'strengthen_extraction':
            result['strict_extraction'] = True
        elif action == 'decompose_prompt':
            result = self._decompose_prompt(result)
        elif action == 'add_env_prefix_filter':
            result['env_prefix'] = 'APP_'
        elif action == 'fix_entropy_formula':
            result['fix_entropy'] = True
        elif action == 'fix_cli_args':
            result['fix_cli'] = True
        elif action == 'fix_fstring_braces':
            result['fix_fstrings'] = True
        elif action == 'fix_indentation':
            result['fix_indent'] = True
        elif action == 'add_missing_import':
            result['add_imports'] = True
        elif action == 'fix_type_annotations':
            result['fix_types'] = True
        
        result['mitigation_applied'] = action
        return result
    
    def _split_prompt(self, context: Dict) -> Dict:
        """Split a complex prompt into subtasks."""
        prompt = context.get('prompt', '')
        parts = re.split(r'\s+(?:and|then|also|plus|additionally)\s+', prompt, flags=re.IGNORECASE)
        if len(parts) == 1:
            parts = re.split(r'\d+\.\s+', prompt)
        if len(parts) > 1:
            context['subtasks'] = [p.strip() for p in parts if p.strip()]
            context['prompt'] = parts[0].strip()
            context['split'] = True
        return context
    
    def _decompose_prompt(self, context: Dict) -> Dict:
        """Decompose prompt into sequential steps."""
        prompt = context.get('prompt', '')
        steps = re.findall(r'\d+\.\s*([^.\n]+)', prompt)
        if not steps:
            steps = re.findall(r'[-\*]\s*([^.\n]+)', prompt)
        if len(steps) > 1:
            context['steps'] = steps
            context['prompt'] = steps[0]
            context['decomposed'] = True
        return context
    
    def get_optimized_prompt(self, category: str, base_prompt: str) -> str:
        """Get an optimized prompt template for a category."""
        if category in self.prompt_patterns:
            pattern = self.prompt_patterns[category]
            if pattern.success_count > 3:
                return f"{pattern.template}\n\n{base_prompt}"
        return base_prompt
    
    def get_stats(self) -> Dict:
        """Get comprehensive learning statistics."""
        total = len(self.records)
        successful = sum(1 for r in self.records if r.success)
        failed = total - successful
        
        avg_quality = statistics.mean([r.code_quality_score for r in self.records]) if self.records else 0
        avg_tokens = statistics.mean([r.tokens_used for r in self.records]) if self.records else 0
        avg_duration = statistics.mean([r.duration_seconds for r in self.records]) if self.records else 0
        
        category_stats = defaultdict(lambda: {'total': 0, 'success': 0})
        for r in self.records:
            category_stats[r.prompt_category]['total'] += 1
            if r.success:
                category_stats[r.prompt_category]['success'] += 1
        
        # Fix effectiveness stats
        fix_stats = {}
        for fix, counts in self.fix_effectiveness.items():
            fix_stats[fix] = {
                "success_rate": counts["success"] / max(counts["total"], 1),
                "total_applied": counts["total"]
            }
        
        return {
            'total_generations': total,
            'successful': successful,
            'failed': failed,
            'success_rate': successful / max(total, 1),
            'avg_code_quality': avg_quality,
            'avg_tokens': avg_tokens,
            'avg_duration': avg_duration,
            'rules_learned': len(self.mitigation_rules),
            'patterns_learned': len(self.prompt_patterns),
            'category_stats': {k: v for k, v in category_stats.items()},
            'top_errors': dict(sorted(self.error_signatures.items(), key=lambda x: x[1], reverse=True)[:10]),
            'top_success_patterns': dict(sorted(self.success_signatures.items(), key=lambda x: x[1], reverse=True)[:10]),
            'fix_effectiveness': fix_stats,
            'total_mitigations_applied': sum(r.times_applied for r in self.mitigation_rules),
            'mitigation_success_rate': sum(r.times_successful for r in self.mitigation_rules) / 
                max(sum(r.times_applied for r in self.mitigation_rules), 1),
            'rule_effectiveness': [
                {
                    'pattern': r.pattern,
                    'action': r.action,
                    'confidence': r.confidence,
                    'applied': r.times_applied,
                    'success_rate': r.times_successful / max(r.times_applied, 1),
                    'effectiveness': r.confidence * (r.times_successful / max(r.times_applied, 1))
                }
                for r in sorted(self.mitigation_rules, key=lambda x: x.confidence * (x.times_successful / max(x.times_applied, 1)), reverse=True)
            ]
        }


# Global instance
_engine: Optional[SelfImprovingEngine] = None


def get_engine() -> SelfImprovingEngine:
    global _engine
    if _engine is None:
        _engine = SelfImprovingEngine()
    return _engine


def record_generation(prompt: str, success: bool, error: Any = None, 
                     code: str = None, fixes: List[str] = None, iterations: int = 1,
                     tokens: int = 0, duration: float = 0):
    """Convenience function to record a generation."""
    prompt_hash = hashlib.md5(prompt.encode()).hexdigest()[:8]
    record = GenerationRecord(
        timestamp=datetime.now().isoformat(),
        prompt_hash=prompt_hash,
        prompt=prompt[:500],
        success=success,
        error_type=type(error).__name__ if error else None,
        error_message=str(error)[:500] if error else None,
        generated_code=code[:2000] if code else None,
        fixes_applied=fixes or [],
        iterations=iterations,
        tokens_used=tokens,
        duration_seconds=duration
    )
    get_engine().record_generation(record)


def get_mitigations(prompt: str, error: str = '', code: str = '', duration: float = 0,
                   tokens: int = 0, iteration: int = 1) -> List[MitigationRule]:
    """Get applicable mitigations for current context."""
    return get_engine().get_applicable_mitigations(prompt, error, code, duration, tokens, iteration)


def apply_mitigation(action: str, context: Dict) -> Dict:
    """Apply a mitigation action."""
    return get_engine().apply_mitigation(action, context)


def get_learning_stats() -> Dict:
    """Get current learning statistics."""
    return get_engine().get_stats()


def get_optimized_prompt(category: str, base_prompt: str) -> str:
    """Get category-optimized prompt."""
    return get_engine().get_optimized_prompt(category, base_prompt)
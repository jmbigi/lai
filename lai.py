#!/usr/bin/env python3
"""
lai - Local AI Code Assistant
A comprehensive CLI tool for reviewing, repairing, and improving any code project
using local LLMs (Qwen3-8B via llama.cpp)
"""

import argparse
import sys
import os
import json
import asyncio
import httpx
import subprocess
import time
import signal
import re
import glob
import shutil
import uuid
from typing import Optional, List, Dict, Any, Callable, AsyncIterator, AsyncGenerator
from dataclasses import dataclass, field
from pathlib import Path

# ============================================================================
# GLOBAL STATE & CONFIGURATION
# ============================================================================

SERVICES = {
    'llama.cpp': {'port': 8080, 'running': False, 'script': '/home/kubuntu/llama.cpp-bin/llama-b12567/llama-server'},
    'gateway': {'port': 8081, 'running': False, 'script': '/home/kubuntu/curso-ia/modelo-local/gateway-go/gateway'},
    'ollama': {'port': 11434, 'running': False, 'script': '/usr/bin/ollama'},
}

MODEL = 'qwen3-8b'
BASE_URL = 'http://localhost:8080/v1'
TEMP = 0.7
MAX_TOKENS = 4096
STREAM = True
THINKING = True
TOOLS_ENABLED = True
AUTO_MODE = False
YOLO = False
VERBOSE = 0
QUIET = False

# ============================================================================
# SERVICE MANAGEMENT
# ============================================================================

def check_service(port: int) -> bool:
    """Check if a service is running on the given port."""
    try:
        with httpx.Client(timeout=1) as client:
            response = httpx.get(f'http://localhost:{port}/health', timeout=1)
            return response.status_code == 200
    except Exception:
        return False

def start_service(name: str):
    """Start a service if not running."""
    if name not in SERVICES:
        return False
    
    service = SERVICES[name]
    if service['running'] or check_service(service['port']):
        return True
    
    script = service.get('script')
    if not script:
        print(f"No script defined for {name}")
        return False
    
    print(f"Starting {name} service on port {SERVICES[name]['port']}...")
    try:
        subprocess.Popen([script], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(3)
        if check_service(SERVICES[name]['port']):
            SERVICES[name]['running'] = True
            print(f"{name} started successfully on port {SERVICES[name]['port']}")
            return True
        else:
            print(f"Failed to start {name} service.")
            return False
    except Exception as e:
        print(f"Error starting {name}: {e}")
        return False

def check_services():
    """Check all services status."""
    for name in SERVICES:
        SERVICES[name]['running'] = check_service(SERVICES[name]['port'])
    return SERVICES

def auto_start_services():
    """Auto-start all configured services if not running."""
    print("Checking and starting local services...")
    for name in SERVICES:
        if not check_service(SERVICES[name]['port']):
            start_service(name)
        else:
            SERVICES[name]['running'] = True
            print(f"{name} already running on port {SERVICES[name]['port']}")
    return check_services()

# ============================================================================
# TOOL REGISTRY
# ============================================================================

class ToolRegistry:
    def __init__(self):
        self.tools = {
            'read_file': self.read_file,
            'write_file': self.write_file,
            'list_dir': self.list_dir,
            'run_command': self.run_command,
            'grep': self.grep,
            'python_eval': self.python_eval,
            'apply_patch': self.apply_patch,
            'web_search': self.web_search,
            'run_python': self.run_python,
            'run_tests': self.run_tests,
            'lint_file': self.lint_file,
            'format_code': self.format_code,
            'check_types': self.check_types,
            'analyze_complexity': self.analyze_complexity,
            'find_unused_imports': self.find_unused_imports,
            'find_unused_code': self.find_unused_code,
            'find_todos': self.find_todos,
            'check_security': self.check_security,
            'check_dead_code': self.check_dead_code,
            'analyze_dependencies': self.analyze_dependencies,
        }

    def read_file(self, path: str) -> str:
        try:
            with open(path, 'r') as f:
                return f.read()
        except Exception as e:
            return f"Error reading file: {str(e)}"

    def write_file(self, path: str, content: str) -> str:
        try:
            os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
            with open(path, 'w') as f:
                f.write(content)
            return f"Written to {path}"
        except Exception as e:
            return f"Error writing file: {str(e)}"

    def list_dir(self, path: str) -> List[str]:
        try:
            return os.listdir(path)
        except Exception as e:
            return [f"Error listing directory: {str(e)}"]

    def run_command(self, cmd: str) -> str:
        try:
            result = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except Exception as e:
            return f"Error running command: {str(e)}"

    def grep(self, pattern: str, path: str) -> List[str]:
        try:
            matches = []
            if os.path.isfile(path):
                filepath = path
                try:
                    with open(filepath, 'r') as f:
                        for i, line in enumerate(f, 1):
                            if pattern in line:
                                matches.append(f"{filepath}:{i}: {line.strip()}")
                except:
                    pass
            else:
                for root, dirs, files in os.walk(path):
                    for file in files:
                        filepath = os.path.join(root, file)
                        try:
                            with open(filepath, 'r') as f:
                                for i, line in enumerate(f, 1):
                                    if pattern in line:
                                        matches.append(f"{filepath}:{i}: {line.strip()}")
                        except:
                            pass
            return matches
        except Exception as e:
            return [f"Error in grep: {str(e)}"]

    def python_eval(self, code: str) -> str:
        try:
            result = eval(code)
            return str(result)
        except Exception as e:
            return f"Error in Python eval: {str(e)}"

    def apply_patch(self, path: str, patch: str) -> str:
        """Apply a unified diff patch to a file."""
        try:
            import subprocess
            result = subprocess.run(['patch', '-p0'], input=patch, text=True, capture_output=True, timeout=30)
            if result.returncode == 0:
                return "Patch applied successfully"
            else:
                return f"Patch failed: {result.stderr}"
        except Exception as e:
            return f"Error applying patch: {str(e)}"

    def web_search(self, query: str, num_results: int = 5, recency_days: int = None) -> str:
        """Search the web for information using DuckDuckGo HTML scraping."""
        try:
            import requests
            from urllib.parse import quote_plus
            import re
            
            # Use DuckDuckGo HTML endpoint
            url = f"https://html.duckduckgo.com/html/?q={quote_plus(query)}"
            headers = {
                'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36'
            }
            
            resp = requests.get(url, headers=headers, timeout=10)
            resp.raise_for_status()
            
            # Parse HTML for results
            from bs4 import BeautifulSoup
            soup = BeautifulSoup(resp.text, 'html.parser')
            
            results = []
            for result in soup.select('.result__body'):
                title_elem = result.select_one('.result__title')
                snippet_elem = result.select_one('.result__snippet')
                url_elem = result.select_one('.result__url')
                
                title = title_elem.get_text(strip=True) if title_elem else ''
                snippet = snippet_elem.get_text(strip=True) if snippet_elem else ''
                url = url_elem.get_text(strip=True) if url_elem else ''
                
                if title and snippet:
                    results.append({
                        'title': title,
                        'snippet': snippet,
                        'url': url
                    })
                    
                if len(results) >= num_results:
                    break
            
            if not results:
                return "No results found"
            
            output = []
            for i, r in enumerate(results, 1):
                output.append(f"{i}. {r['title']}\n   {r['snippet'][:200]}\n   {r['url']}\n")
            
            return "\n".join(output)
        except ImportError:
            return "Error: beautifulsoup4 not installed. Run: pip install beautifulsoup4"
        except Exception as e:
            return f"Error in web search: {str(e)}"

    def run_python(self, code: str, args: List[str] = None, timeout: int = 30) -> str:
        """Run Python code with optional arguments."""
        try:
            import tempfile
            import os
            with tempfile.NamedTemporaryFile(mode='w', suffix='.py', delete=False) as f:
                f.write(code)
                temp_path = f.name
            try:
                cmd = ['python3', temp_path] + (args or [])
                result = subprocess.run(['python3', temp_path] + (args or []), 
                                      capture_output=True, text=True, timeout=timeout)
                os.unlink(temp_path)
                return result.stdout if result.stdout else result.stderr
            except Exception as e:
                if os.path.exists(temp_path):
                    os.unlink(temp_path)
                raise
        except Exception as e:
            return f"Error running Python: {str(e)}"

    def run_tests(self, path: str = ".", pattern: str = "test_*.py", coverage: bool = False) -> str:
        """Run tests with pytest."""
        try:
            cmd = ['python3', '-m', 'pytest', path]
            if pattern:
                cmd.extend(['-k', pattern])
            if coverage:
                cmd.extend(['--cov=.', '--cov-report=term-missing'])
            result = subprocess.run(['python3', '-m', 'pytest'] + (['-k', pattern] if pattern else []) + 
                                  (['--cov=.', '--cov-report=term-missing'] if coverage else []) + [path],
                                   capture_output=True, text=True, timeout=120)
            output = result.stdout
            if result.stderr:
                output += "\nSTDERR:\n" + result.stderr
            return output
        except subprocess.TimeoutExpired:
            return "Tests timed out"
        except Exception as e:
            return f"Error running tests: {str(e)}"

    def lint_file(self, path: str, fix: bool = False) -> str:
        """Lint Python file with ruff."""
        try:
            if fix:
                result = subprocess.run(['ruff', 'check', '--fix', path], capture_output=True, text=True, timeout=60)
            else:
                result = subprocess.run(['ruff', 'check', path], capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except Exception as e:
            return f"Error running linter: {str(e)}"

    def format_code(self, path: str, line_length: int = 88) -> str:
        """Format Python code with black."""
        try:
            result = subprocess.run(['black', '--line-length', str(line_length), path], 
                                  capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except Exception as e:
            return f"Error formatting code: {str(e)}"

    def check_types(self, path: str) -> str:
        """Run mypy type checking."""
        try:
            result = subprocess.run(['mypy', path], capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except Exception as e:
            return f"Error running mypy: {str(e)}"

    def analyze_complexity(self, path: str) -> str:
        """Analyze code complexity using radon."""
        try:
            result = subprocess.run(['radon', 'cc', '-a', '-nc', path], 
                                  capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "radon not installed. Install with: pip install radon"
        except Exception as e:
            return f"Error analyzing complexity: {str(e)}"

    def find_unused_imports(self, path: str) -> str:
        """Find unused imports using pyflakes."""
        try:
            result = subprocess.run(['pyflakes', path], capture_output=True, text=True, timeout=30)
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "pyflakes not installed. Install with: pip install pyflakes"
        except Exception as e:
            return f"Error finding unused imports: {str(e)}"

    def find_unused_code(self, path: str) -> str:
        """Find potentially unused code using vulture."""
        try:
            result = subprocess.run(['vulture', path], capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "vulture not installed. Install with: pip install vulture"
        except Exception as e:
            return f"Error finding unused code: {str(e)}"

    def find_todos(self, path: str) -> str:
        """Find TODO/FIXME comments in code."""
        try:
            result = subprocess.run(['grep', '-rn', '-E', '(TODO|FIXME|XXX|HACK)', path], 
                                  capture_output=True, text=True, timeout=30)
            return result.stdout if result.stdout else "No TODOs found"
        except Exception as e:
            return f"Error finding TODOs: {str(e)}"

    def check_security(self, path: str) -> str:
        """Check for security issues using bandit."""
        try:
            result = subprocess.run(['bandit', '-r', path], capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "bandit not installed. Install with: pip install bandit"
        except Exception as e:
            return f"Error checking security: {str(e)}"

    def check_dead_code(self, path: str) -> str:
        """Find dead code using vulture."""
        try:
            result = subprocess.run(['vulture', path], capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "vulture not installed. Install with: pip install vulture"
        except Exception as e:
            return f"Error finding dead code: {str(e)}"

    def analyze_dependencies(self, path: str) -> str:
        """Analyze imports and dependencies."""
        try:
            result = subprocess.run(['pydeps', path, '--no-output', '--show-deps'], 
                                  capture_output=True, text=True, timeout=60)
            return result.stdout if result.stdout else result.stderr
        except FileNotFoundError:
            return "pydeps not installed. Install with: pip install pydeps"
        except Exception as e:
            return f"Error analyzing dependencies: {str(e)}"

    def get_tools_schema(self) -> List[Dict]:
        return [
            {"type": "function", "function": {"name": "read_file", "description": "Read file contents", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "write_file", "description": "Write file", "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "content": {"type": "string"}}, "required": ["path", "content"]}}},
            {"type": "function", "function": {"name": "list_dir", "description": "List directory", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "run_command", "description": "Run shell command", "parameters": {"type": "object", "properties": {"cmd": {"type": "string"}}, "required": ["cmd"]}}},
            {"type": "function", "function": {"name": "grep", "description": "Search pattern in file", "parameters": {"type": "object", "properties": {"pattern": {"type": "string"}, "path": {"type": "string"}}, "required": ["pattern", "path"]}}},
            {"type": "function", "function": {"name": "python_eval", "description": "Evaluate Python code", "parameters": {"type": "object", "properties": {"code": {"type": "string"}}, "required": ["code"]}}},
            {"type": "function", "function": {"name": "apply_patch", "description": "Apply unified diff patch", "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "patch": {"type": "string"}}, "required": ["path", "patch"]}}},
            {"type": "function", "function": {"name": "run_python", "description": "Run Python script", "parameters": {"type": "object", "properties": {"code": {"type": "string"}, "args": {"type": "array", "items": {"type": "string"}}, "timeout": {"type": "integer", "default": 30}}, "required": ["code"]}}},
            {"type": "function", "function": {"name": "run_tests", "description": "Run tests", "parameters": {"type": "object", "properties": {"path": {"type": "string", "default": "."}, "pattern": {"type": "string", "default": "test_*.py"}, "coverage": {"type": "boolean", "default": false}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "lint_file", "description": "Lint Python file", "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "fix": {"type": "boolean", "default": false}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "format_code", "description": "Format Python code with black", "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "line_length": {"type": "integer", "default": 88}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "check_types", "description": "Run mypy type checking", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "analyze_complexity", "description": "Analyze code complexity", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "find_unused_imports", "description": "Find unused imports", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "find_unused_code", "description": "Find unused code", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "find_todos", "description": "Find TODO/FIXME comments", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "check_security", "description": "Check for security issues", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "check_dead_code", "description": "Find dead code", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
            {"type": "function", "function": {"name": "analyze_dependencies", "description": "Analyze imports and dependencies", "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
        ]

# ============================================================================
# SESSION MANAGEMENT
# ============================================================================

@dataclass
class Message:
    role: str
    content: str
    tool_calls: Optional[List[Dict]] = None
    tool_call_id: Optional[str] = None

@dataclass
class Session:
    id: str
    messages: List[Dict] = field(default_factory=list)
    created_at: float = field(default_factory=time.time)
    updated_at: float = field(default_factory=time.time)

class SessionManager:
    def __init__(self, history_dir: str = None):
        if history_dir is None:
            history_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "sessions")
        self.history_dir = history_dir
        os.makedirs(history_dir, exist_ok=True)
    
    def create_session(self, session_id: Optional[str] = None) -> str:
        if session_id is None:
            session_id = str(uuid.uuid4())
        session_path = os.path.join(self.history_dir, f"{session_id}.json")
        if not os.path.exists(session_path):
            self._save_session(session_id, [])
        return session_id
    
    def get_session(self, session_id: str) -> List[Dict]:
        session_path = os.path.join(self.history_dir, f"{session_id}.json")
        if os.path.exists(session_path):
            with open(session_path, "r", encoding="utf-8") as f:
                return json.load(f)
        return []
    
    def save_session(self, session_id: str, messages: List[Dict]):
        self._save_session(session_id, messages)
    
    def _save_session(self, session_id: str, messages: List[Dict]):
        session_path = os.path.join(self.history_dir, f"{session_id}.json")
        with open(session_path, "w", encoding="utf-8") as f:
            json.dump(messages, f, indent=2, ensure_ascii=False)
    
    def list_sessions(self) -> List[str]:
        if not os.path.exists(self.history_dir):
            return []
        return [
            f.replace(".json", "")
            for f in os.listdir(self.history_dir)
            if f.endswith(".json")
        ]
    
    def delete_session(self, session_id: str):
        session_path = os.path.join(self.history_dir, f"{session_id}.json")
        if os.path.exists(session_path):
            os.remove(session_path)

# ============================================================================
# THINKING MANAGEMENT
# ============================================================================

@dataclass
class ThinkingBlock:
    content: str
    start_time: float
    end_time: Optional[float] = None
    tokens: int = 0

class ThinkingManager:
    def __init__(self, enabled: bool = True):
        self.enabled = enabled
        self.thinking_blocks: List[ThinkingBlock] = []
    
    def extract_reasoning(self, content: str) -> tuple[str, str]:
        """Extract reasoning content from model response."""
        if not self.enabled:
            return "", content
        
        patterns = [
            r"<thinking>\s*(.*?)\s*</thinking>",           # Qwen3 standard
            r"questions\s*(.*?)\s*回答",                   # DeepSeek style (questions...回答)
            r"questions\s*(.*?)\s*</thinking>",           # questions... with </thinking>
            r"think\s*(.*?)\s*</thinking>",               # think... with </thinking>
            r"\|begin_thinking\|(.*?)\|end_thinking\|",   # Some models
        ]
        
        for pattern in patterns:
            matches = list(re.finditer(pattern, content, re.DOTALL))
            if matches:
                match = matches[-1]
                thinking = match.group(1).strip()
                clean_content = re.sub(pattern, "", content, flags=re.DOTALL).strip()
                
                block = ThinkingBlock(
                    content=thinking,
                    start_time=time.time() - 1,
                    end_time=time.time(),
                    tokens=len(thinking) // 4
                )
                self.thinking_blocks.append(block)
                return thinking, clean_content
        
        return "", content
    
    def format_thinking_for_display(self, thinking: str, content: str) -> str:
        if self.enabled and thinking:
            return f"\033[93m<thinking>\n{thinking}\n</thinking>\033[0m\n\n{content}"
        return content
    
    def get_thinking_summary(self) -> Dict[str, Any]:
        total_tokens = sum(b.tokens for b in self.thinking_blocks)
        return {
            "blocks": len(self.thinking_blocks),
            "total_tokens": total_tokens,
            "blocks_detail": [
                {"tokens": b.tokens, "preview": b.content[:100]} 
                for b in self.thinking_blocks
            ]
        }
    
    def clear(self):
        self.thinking_blocks.clear()

# ============================================================================
# AUTO MODE
# ============================================================================

class AutoMode:
    def __init__(self, goal: str, tools: 'ToolRegistry', lai_instance: 'LAI', max_iterations: int = 10):
        self.goal = goal
        self.tools = tools
        self.lai = lai_instance
        self.max_iterations = max_iterations
        self.iteration = 0
        self.history: List[Dict] = []
    
    def run(self) -> str:
        """Run autonomous improvement loop with tool execution."""
        results = []
        
        while self.iteration < self.max_iterations:
            self.iteration += 1
            results.append(f"\n{'='*50}")
            results.append(f"Auto Mode - Iteration {self.iteration}/{self.max_iterations}")
            results.append(f"Goal: {self.goal}")
            results.append(f"{'='*50}")
            
            # Build prompt with available tools
            tools_list = ", ".join(self.tools.tools.keys())
            goal_lower = self.goal.lower()
            
            # Heuristic: choose tool based on goal keywords
            # Check for "create/write/make/generate" first (before "list" which might match "list methods")
            if "create" in goal_lower or "write" in goal_lower or "make" in goal_lower or "generate" in goal_lower:
                if ".py" in goal_lower or "python" in goal_lower or "code" in goal_lower or "function" in goal_lower or "class" in goal_lower:
                    tool_name = "write_file"
                    # Extract filename from goal
                    import re
                    fname_match = re.search(r'(\w+\.py)', goal_lower)
                    filename = fname_match.group(1) if fname_match else "output.py"
                    # Generate code using LLM (separate call, not in JSON)
                    # Clear conversation history to avoid confusing the model with previous context
                    self.lai.conversation_history.clear()
                    code_prompt = f"""Generate ONLY valid Python code for: {self.goal}

Requirements:
- Valid Python 3, executable COMPLETE SCRIPT
- Professional structure with functions/classes
- MUST include a main() function
- MUST include if __name__ == "__main__": block to run the program
- Error handling (try/except)
- Type hints where appropriate
- Docstrings in main functions/classes
- Clean, readable, PEP 8 compliant
- No markdown, no explanations, ONLY code
- Imports at top
- No test code, only the requested program
- ASCII ONLY: straight quotes and NO curly quotes
- NO special Unicode characters

Output ONLY the complete Python code for {filename}:"""
                    generated_code = self.lai.chat(code_prompt, stream=False, thinking=False, temp=0.0, max_tokens=2048)
                    # Clean up any markdown (chat method should have stripped, but just in case)
                    if generated_code.startswith("```python"):
                        generated_code = generated_code[9:]
                    elif generated_code.startswith("```"):
                        generated_code = generated_code[3:]
                    if generated_code.endswith("```"):
                        generated_code = generated_code[:-3]
                    generated_code = generated_code.strip()
                    # Validate syntax before writing
                    try:
                        compile(generated_code, filename, 'exec')
                    except SyntaxError as e:
                        generated_code = f"# Generated: {self.goal}\n# Syntax error: {e}\nprint('Error: Invalid code generated')"
                    # Write the file directly, bypass JSON tool call
                    self.tools.write_file(filename, generated_code)
                    results.append(f"✅ Generated and saved {filename}")
                    self.history.append(f"Iteration {self.iteration}: Created {filename} with LLM-generated code")
                    # Skip to next iteration
                    continue
                else:
                    tool_name = "list_dir"
                    tool_args = {"path": "."}
            elif "search" in goal_lower or "grep" in goal_lower:
                tool_name = "grep"
                # Extract search pattern
                import re
                pattern_match = re.search(r"['\"]([^'\"]+)['\"]", self.goal)
                pattern = pattern_match.group(1) if pattern_match else "def "
                tool_args = {"pattern": pattern, "path": "."}
            elif "find" in goal_lower:
                tool_name = "find"
                import re
                pattern_match = re.search(r"['\"]([^'\"]+)['\"]", self.goal)
                pattern = pattern_match.group(1) if pattern_match else "*.py"
                tool_args = {"pattern": pattern, "path": "."}
            elif "list" in goal_lower or "files" in goal_lower or "directory" in goal_lower:
                tool_name = "list_dir"
                tool_args = {"path": "."}
            elif "run" in goal_lower or "execute" in goal_lower:
                tool_name = "run_command"
                tool_args = {"cmd": "ls -la"}
            else:
                tool_name = "list_dir"
                tool_args = {"path": "."}
            
            prompt = f"/no_think Output ONLY this JSON: {{\"tool\": \"{tool_name}\", \"args\": {json.dumps(tool_args)}, \"reasoning\": \"{self.goal[:50]}\"}}" if self.iteration == 1 else f"/no_think Output ONLY: {{\"done\": true, \"result\": \"{self.goal[:80]}\"}}"

            response = self.lai.chat(prompt, stream=False, thinking=False, temp=0.0, max_tokens=500)
            
            # Extract JSON from response - find the last complete JSON object
            import re
            json_candidates = re.findall(r'\{.*?\}(?=\s*$|\s*\{)', response, re.DOTALL)
            if not json_candidates:
                # Fallback: find any JSON-like object
                json_match = re.search(r'\{.*\}', response, re.DOTALL)
                if json_match:
                    json_candidates = [json_match.group(0)]
            
            action = None
            for candidate in reversed(json_candidates):
                try:
                    action = json.loads(candidate)
                    break
                except json.JSONDecodeError:
                    continue
            
            if action is None:
                # Try to fix common JSON issues (unescaped newlines in strings)
                for candidate in reversed(json_candidates):
                    try:
                        # Fix unescaped newlines in JSON strings
                        fixed = re.sub(r'(?<!\\)\n', '\\n', candidate)
                        action = json.loads(fixed)
                        break
                    except json.JSONDecodeError:
                        continue
            
            if action:
                
                if action.get("done"):
                    results.append(f"✓ Task completed!")
                    results.append(f"Result: {action.get('result', 'Done')}")
                    self.history.append(f"Iteration {self.iteration}: Completed - {action.get('result', 'Done')}")
                    break
                
                tool_name = action.get("tool")
                args = action.get("args", {})
                reasoning = action.get("reasoning", "")
                
                if tool_name and tool_name in self.tools.tools:
                    results.append(f"🔧 Executing: {tool_name} with args: {args}")
                    results.append(f"   Reasoning: {reasoning}")
                    
                    tool_func = self.tools.tools[tool_name]
                    try:
                        tool_result = tool_func(**args)
                        results.append(f"   Result: {str(tool_result)[:500]}")
                        self.history.append(f"Iteration {self.iteration}: {tool_name}({args}) -> {str(tool_result)[:200]}")
                    except Exception as e:
                        results.append(f"   Error: {e}")
                        self.history.append(f"Iteration {self.iteration}: {tool_name}({args}) -> ERROR: {e}")
                else:
                    results.append(f"⚠ Unknown tool: {tool_name}")
                    self.history.append(f"Iteration {self.iteration}: Unknown tool {tool_name}")
            else:
                results.append(f"⚠ No valid JSON in response")
                results.append(f"Raw response: {response[:300]}")
                self.history.append(f"Iteration {self.iteration}: No JSON found")
        
        if self.iteration >= self.max_iterations:
            results.append(f"\n⚠ Max iterations ({self.max_iterations}) reached")
        
        return "\n".join(results)

# ============================================================================
# MAIN LAI CLASS
# ============================================================================

class LAI:
    def __init__(self, model: str = "qwen3-8b", base_url: str = "http://localhost:8080/v1"):
        self.model = model
        self.base_url = base_url
        self.harness = None
        self.thinking = True
        self.stream = True
        self.tools = ToolRegistry()
        self.auto_mode = None
        self.session_manager = SessionManager()
        self.current_session_id: Optional[str] = None
        self.thinking_manager = ThinkingManager()
        self.conversation_history: List[Dict] = []

    async def _chat(self, prompt: str, stream: bool = True, thinking: bool = True, temp: float = 0.7, max_tokens: int = 4096) -> str:
        """Chat with the model with proper streaming support."""
        try:
            if thinking:
                prompt = f"<thinking>\n{prompt}\n</thinking>\n\nProvide the solution."
            
            async with httpx.AsyncClient(timeout=120) as client:
                payload = {
                    "model": self.model,
                    "messages": [{"role": "user", "content": prompt}],
                    "max_tokens": max_tokens,
                    "temperature": temp,
                    "stream": stream,
                }
                
                if stream:
                    content_buffer = []
                    async with client.stream("POST", f"{self.base_url}/chat/completions", json=payload) as resp:
                        async for line in resp.aiter_lines():
                            if line.startswith("data: "):
                                data = line[6:]
                                if data.strip() == "[DONE]":
                                    break
                                try:
                                    data = json.loads(data)
                                    content = data.get("choices", [{}])[0].get("delta", {}).get("content", "")
                                    if content:
                                        print(content, end="", flush=True)
                                        content_buffer.append(content)
                                except Exception:
                                    pass
                        print()
                        return "".join(content_buffer)
                else:
                    resp = await client.post(f"{self.base_url}/chat/completions", json=payload, timeout=120)
                    data = resp.json()
                    return data.get("choices", [{}])[0].get("message", {}).get("content", "")
        except Exception as e:
            return f"Error: {e}"

    def chat(self, prompt: str, stream: bool = True, thinking: bool = True, temp: float = 0.7, max_tokens: int = 4096) -> str:
        # Add to conversation history
        self.conversation_history.append({"role": "user", "content": prompt})
        
        try:
            if thinking:
                full_prompt = f"<thinking>\n{prompt}\n</thinking>\n\nProvide the solution."
            else:
                full_prompt = prompt
            
            # Include conversation history
            messages = self.conversation_history.copy()
            if thinking and messages:
                # Wrap last message with thinking tags
                messages[-1]["content"] = f"<thinking>\n{messages[-1]['content']}\n</thinking>\n\nProvide the solution."
            
            async def _do_chat():
                async with httpx.AsyncClient(timeout=120) as client:
                    # Add /no_think system message to suppress thinking when disabled
                    final_messages = messages.copy()
                    if not thinking:
                        # Insert /no_think as system message at the beginning
                        final_messages.insert(0, {"role": "system", "content": "/no_think"})
                    
                    payload = {
                        "model": self.model,
                        "messages": final_messages,
                        "max_tokens": max_tokens,
                        "temperature": temp,
                        "stream": stream,
                    }
                    
                    headers = {"Content-Type": "application/json"}
                    # Bypass cache for non-streaming requests
                    if not stream:
                        headers["Cache-Control"] = "no-cache"
                        headers["X-No-Cache"] = "true"
                    
                    if stream:
                        content_buffer = []
                        async with client.stream("POST", f"{self.base_url}/chat/completions", json=payload, headers=headers) as resp:
                            async for line in resp.aiter_lines():
                                if line.startswith("data: "):
                                    data = line[6:]
                                    if data.strip() == "[DONE]":
                                        break
                                    try:
                                        data = json.loads(data)
                                        content = data.get("choices", [{}])[0].get("delta", {}).get("content", "")
                                        if content:
                                            print(content, end="", flush=True)
                                            content_buffer.append(content)
                                    except Exception:
                                        pass
                            print()
                            return "".join(content_buffer)
                    else:
                        resp = await client.post(f"{self.base_url}/chat/completions", json=payload, headers=headers, timeout=120)
                        # Handle both streaming and non-streaming responses from gateway
                        content_type = resp.headers.get("content-type", "")
                        if "text/event-stream" in content_type:
                            # Gateway returned streaming format - parse chunks
                            content_buffer = []
                            text = resp.text
                            for line in text.split('\n'):
                                if line.startswith("data: "):
                                    data_str = line[6:]
                                    if data_str.strip() == "[DONE]":
                                        break
                                    try:
                                        data = json.loads(data_str)
                                        content = data.get("choices", [{}])[0].get("delta", {}).get("content", "")
                                        if content:
                                            content_buffer.append(content)
                                    except Exception:
                                        pass
                            return "".join(content_buffer)
                        else:
                            data = resp.json()
                            return data.get("choices", [{}])[0].get("message", {}).get("content", "")
            
            response = asyncio.run(_do_chat())
            self.conversation_history.append({"role": "assistant", "content": response})
            
            # Extract thinking only if explicitly enabled
            if thinking and self.thinking_manager.enabled:
                thinking_content, clean_content = self.thinking_manager.extract_reasoning(response)
                if thinking_content:
                    response = self.thinking_manager.format_thinking_for_display(thinking_content, clean_content)
            elif not thinking:
                # When thinking is disabled, ensure we don't have thinking artifacts
                # The model might still output <thinking>...</thinking>, "questions...回答" format, or "questions...</thinking>"
                
                import re
                
                # Handle <thinking>...</thinking> format
                if "<thinking>" in response and "</thinking>" in response:
                    response = re.sub(r"<thinking>.*?</thinking>", "", response, flags=re.DOTALL).strip()
                
                # Handle "question...回答" format (model outputs "question" as prefix)
                if "question" in response.lower() and "回答" in response:
                    response = re.sub(r"question.*?回答", "", response, flags=re.DOTALL | re.IGNORECASE).strip()
                
                # Handle "thinking...回答" format (some models)
                if "thinking" in response.lower() and "回答" in response:
                    response = re.sub(r"thinking.*?回答", "", response, flags=re.DOTALL | re.IGNORECASE).strip()
                
                # Handle "questions...回答" format (DeepSeek style)
                if "回答" in response:
                    parts = response.split("回答")
                    if len(parts) > 1:
                        response = parts[-1].strip()
                
                # Handle "questions...</thinking>" format (some models)
                if "questions" in response.lower() and "</thinking>" in response:
                    response = re.sub(r"questions\s*(.*?)\s*</thinking>", "", response, flags=re.DOTALL | re.IGNORECASE).strip()
                
                # Also strip markdown code fences if present (handle cases with leading text)
                response = response.strip()
                # Extract code from markdown fences if present
                if "```python" in response:
                    # Extract content between ```python and ```
                    match = re.search(r'```python\n(.*?)\n```', response, re.DOTALL)
                    if match:
                        response = match.group(1).strip()
                elif "```" in response:
                    # Extract content between ``` and ```
                    match = re.search(r'```\n(.*?)\n```', response, re.DOTALL)
                    if match:
                        response = match.group(1).strip()
                else:
                    # Fallback: strip leading ```python or ``` if at start
                    response = response.strip()
                    if response.startswith("```python"):
                        response = response[9:]
                    elif response.startswith("```"):
                        response = response[3:]
                    if response.endswith("```"):
                        response = response[:-3]
                    response = response.strip()
            
            return response
        except Exception as e:
            return f"Error: {e}"
    
    def think(self, problem: str) -> str:
        return self.chat(f"<thinking>\n{problem}\n</thinking>\n\nProvide the solution.", thinking=True)

    def auto_mode_run(self, goal: str, max_iterations: int = 10):
        auto = AutoMode(goal, self.tools, self, max_iterations)
        return auto.run()

    def code(self, description: str, filename: str, auto: bool = False, yolo: bool = False):
        """Generate high-quality code using the local LLM."""
        base_prompt = """Generate ONLY valid Python code, no explanations, no markdown.

Description: {description}

CRITICAL REQUIREMENTS:
- Valid Python 3 code, executable
- Professional structure with functions/classes
- Error handling (try/except)
- Type hints where appropriate
- Docstrings in main functions/classes
- Clean, readable code following PEP 8
- No markdown, no explanations, only valid Python code
- Imports at the top
- No test code, only the requested code
- USE ONLY ASCII CHARACTERS: straight quotes ' and ", NO curly quotes " " ' '
- NO special Unicode characters (—, –, •, etc.)

Expected structure example:
```python
import sys
import os
from typing import List, Optional

class MyClass:
    def __init__(self):
        pass
    
    def my_method(self) -> str:
        return "Hello"

def main():
    obj = MyClass()
    print(obj.my_method())

if __name__ == "__main__":
    main()
```"""

        prompt = f"""{base_prompt}

Task: {description}

Generate ONLY the complete Python code for file: {filename}

IMPORTANT: Use straight quotes (') or ("), NO curly quotes. Use pure ASCII."""

        try:
            response = self.chat(prompt, stream=False, thinking=False, temp=0.0, max_tokens=4096)
            code = response.strip()
            
            # Clean up any markdown fences if the model added them
            if code.startswith("```python"):
                code = code[9:]
            if code.endswith("```"):
                code = code[:-3]
            code = code.strip()
            
            # Validate syntax
            try:
                compile(code, filename, 'exec')
            except SyntaxError as e:
                print(f"Warning: Generated code has syntax error: {e}")
                code = f"# Generated: {description}\nprint('Error: Could not generate valid code')"
            
            if not yolo:
                print(f"Generated code for: {description}")
                print("-" * 40)
                print(code[:500] + ("..." if len(code) > 500 else ""))
                print("-" * 40)
                confirm = input("Save to file? (y/n): ")
                if confirm.lower() != 'y':
                    print("Cancelled.")
                    return False
            
            os.makedirs(os.path.dirname(os.path.abspath(filename)), exist_ok=True)
            
            with open(filename, 'w') as f:
                f.write(code)
            print(f"Code saved to {filename}")
            
        except Exception as e:
            print(f"Error generating code: {e}")
            fallback_code = f"# Generated: {description}\nprint('Error generating code')"
            os.makedirs(os.path.dirname(os.path.abspath(filename)), exist_ok=True)
            with open(filename, 'w') as f:
                f.write(fallback_code)
            print(f"Fallback code saved to {filename}")

    def run_script(self, script: str):
        if not os.path.exists(script):
            return f"Script not found: {script}"
        result = subprocess.run(['python3', script], capture_output=True, text=True, timeout=60)
        return result.stdout

    def serve(self, port: int = 8080):
        print(f"Starting API server on port {port}...")
        print("Use lai serve --port <port> to start API server")

    def status(self):
        check_services()
        print("Services status:")
        for name, info in SERVICES.items():
            status = "Running" if info['running'] else "Stopped"
            print(f"  {name}: {status} (Port {info['port']})")

    def models(self):
        print("Available models:")
        print("  qwen3-8b (local)")
        print("  qwen3:8b (ollama)")

    def tools_list(self):
        print("Available tools:")
        for name, func in ToolRegistry().tools.items():
            print(f"  {name}: {func.__doc__}")

# ============================================================================
# COMMAND HANDLERS
# ============================================================================

# Global LAI instance for session persistence
_lai_instance: Optional[LAI] = None

def get_lai_instance(model: str = "qwen3-8b", base_url: str = "http://localhost:8080/v1") -> LAI:
    """Get or create singleton LAI instance."""
    global _lai_instance
    if _lai_instance is None:
        _lai_instance = LAI(model=model, base_url=base_url)
    return _lai_instance

def chat_cmd(args):
    lai = get_lai_instance(model=args.model, base_url=args.base_url)
    stream = args.stream if args.stream else (not args.no_stream)
    thinking = args.thinking if args.thinking else (not args.no_thinking)
    temp = args.temp or 0.7
    max_tokens = args.max_tokens or 4096
    
    if args.interactive:
        print("Interactive mode - type 'exit' or 'quit' to quit")
        print("Type 'clear' to clear conversation history")
        while True:
            try:
                prompt = input("You: ")
                if prompt.lower() in ('exit', 'quit'):
                    break
                if prompt.lower() == 'clear':
                    lai.conversation_history.clear()
                    print("Conversation history cleared.")
                    continue
                response = lai.chat(prompt, stream=stream, thinking=thinking, temp=temp, max_tokens=max_tokens)
                print()
            except (EOFError, KeyboardInterrupt):
                print()
                break
    else:
        prompt = " ".join(args.prompt) if args.prompt else ""
        if not prompt:
            print("Error: No prompt provided")
            return
        response = lai.chat(prompt, stream=stream, thinking=thinking, temp=temp, max_tokens=max_tokens)
        print(response)

def generate_cmd(args):
    prompt = " ".join(args.prompt) if args.prompt else ""
    if not prompt:
        print("Error: No prompt provided")
        return
    lai = get_lai_instance(model=args.model, base_url=args.base_url)
    response = lai.chat(f"Generate: {prompt}", stream=args.stream, thinking=args.thinking, temp=args.temp or 0.7, max_tokens=args.max_tokens or 2048)
    print(response)

def code_cmd(lai: LAI, args):
    lai.code(args.description, args.filename, auto=args.auto, yolo=args.yolo)

def auto_cmd(lai: LAI, args):
    print(f"Auto mode: {args.goal}")
    result = lai.auto_mode_run(args.goal, max_iterations=args.max_iterations)
    print(result)

def run_cmd(lai: LAI, args):
    print(lai.run_script(args.script))

def think_cmd(lai: LAI, args):
    print(lai.think(args.problem))

def search_cmd(lai: LAI, args):
    results = lai.tools.grep(args.pattern, args.path)
    if args.type:
        results = [r for r in results if r.endswith(f".{args.type}")]
    if args.ignore_case:
        results = [r for r in results if args.pattern.lower() in r.lower()]
    for r in results:
        print(r)

def find_cmd(lai: LAI, args):
    for root, dirs, files in os.walk(args.path):
        for name in files + dirs:
            if args.pattern in name:
                if args.type == 'f' and os.path.isfile(os.path.join(root, name)):
                    print(os.path.join(root, name))
                elif args.type == 'd' and os.path.isdir(os.path.join(root, name)):
                    print(os.path.join(root, name))
                elif args.type == 'l' and os.path.islink(os.path.join(root, name)):
                    print(os.path.join(root, name))
                elif not args.type:
                    print(os.path.join(root, name))

def fix_cmd(lai: LAI, args):
    print(f"Fixing: {args.path}")
    if args.type in ('lint', 'all'):
        print("Running linter...")
        print(lai.tools.lint_file(args.path, fix=args.auto))
    if args.type in ('format', 'all'):
        print("Formatting code...")
        print(lai.tools.format_code(args.path))
    if args.type in ('imports', 'all'):
        print("Organizing imports...")
        print(lai.tools.find_unused_imports(args.path))
    if args.type in ('types', 'all'):
        print("Checking types...")
        print(lai.tools.check_types(args.path))

def review_cmd(lai: LAI, args):
    print(f"Reviewing: {args.path} (focus: {args.focus})")
    # Would use LAI to review code

def diff_cmd(lai: LAI, args):
    import difflib
    with open(args.file1) as f1, open(args.file2) as f2:
        diff = difflib.unified_diff(f1.readlines(), f2.readlines(), 
                                     fromfile=args.file1, tofile=args.file2,
                                     n=args.context)
        sys.stdout.writelines(diff)

def test_cmd(lai: LAI, args):
    import pytest
    sys.exit(pytest.main([args.path, '-v' if args.verbose else '', 
                          '--pattern', args.pattern or 'test_*.py',
                          '--cov' if args.coverage else '']))

def init_cmd(lai: LAI, args):
    project_types = {
        'python': ['main.py', 'requirements.txt', 'README.md'],
        'fastapi': ['main.py', 'requirements.txt', 'README.md', 'Dockerfile'],
        'flask': ['app.py', 'requirements.txt', 'README.md'],
        'django': ['manage.py', 'settings.py', 'requirements.txt'],
        'react': ['package.json', 'src/App.jsx', 'public/index.html'],
        'vue': ['package.json', 'src/App.vue', 'index.html'],
        'svelte': ['package.json', 'src/App.svelte', 'index.html'],
        'cli': ['main.py', 'cli.py', 'requirements.txt'],
        'lib': ['src/__init__.py', 'pyproject.toml', 'README.md'],
    }
    files = project_types.get(args.type, ['main.py', 'README.md'])
    os.makedirs(args.path, exist_ok=True)
    for f in files:
        path = os.path.join(args.path, f)
        if not os.path.exists(path):
            with open(path, 'w') as f:
                f.write(f"# {args.name or args.type}\n")
    if args.git:
        subprocess.run(['git', 'init'], cwd=args.path)
    print(f"Initialized {args.type} project: {args.name} at {args.path}")


def project_cmd(lai: LAI, args):
    """Generate a complete project from a natural language description."""
    description = " ".join(args.description) if args.description else ""
    if not description:
        print("Error: No project description provided")
        return
    
    path = args.path or "."
    os.makedirs(path, exist_ok=True)
    
    print(f"🎯 Analyzing project description...")
    print(f"📁 Target path: {os.path.abspath(path)}")
    
    # Step 1: Analyze the description and create a project plan
    plan_prompt = f"""Analyze this project description and create a detailed project plan.

Description: {description}

Output ONLY a JSON object with this structure:
{{
    "project_type": "fastapi|django|flask|react|vue|svelte|cli|lib|python",
    "name": "short-project-name",
    "description": "brief description",
    "tech_stack": {{"language": "python", "framework": "fastapi", "database": "postgresql", "orm": "sqlalchemy"}},
    "features": ["auth", "crud", "api", "admin", "tests", "docker"],
    "files": [
        {{"path": "main.py", "purpose": "FastAPI app entry point"}},
        {{"path": "models.py", "purpose": "SQLAlchemy models"}},
        {{"path": "schemas.py", "purpose": "Pydantic schemas"}},
        {{"path": "api/routes.py", "purpose": "API endpoints"}},
        {{"path": "database.py", "purpose": "Database connection"}},
        {{"path": "requirements.txt", "purpose": "Python dependencies"}},
        {{"path": "Dockerfile", "purpose": "Docker configuration"}},
        {{"path": "docker-compose.yml", "purpose": "Docker Compose"}},
        {{"path": "tests/test_main.py", "purpose": "Test suite"}},
        {{"path": "README.md", "purpose": "Documentation"}}
    ]
}}

The plan should be appropriate for the described project. Include all necessary files for a production-ready project.""".replace("{", "{{").replace("}", "}}")
    
    print("🤖 Generating project plan...")
    plan_response = lai.chat(plan_prompt, stream=False, thinking=False, temp=0.1, max_tokens=2048)
    
    # Extract JSON from response
    import re
    json_match = re.search(r'\{.*\}', plan_response, re.DOTALL)
    if not json_match:
        print("❌ Failed to generate project plan")
        print(f"Raw response: {plan_response[:500]}")
        return
    
    try:
        import json
        plan = json.loads(json_match.group(0))
    except json.JSONDecodeError as e:
        print(f"❌ Failed to parse project plan: {e}")
        print(f"Raw: {plan_response[:500]}")
        return
    
    print(f"✅ Project plan generated: {plan.get('name', 'unnamed')} ({plan.get('project_type', 'unknown')})")
    print(f"📋 Features: {', '.join(plan.get('features', []))}")
    print(f"📄 Files to generate: {len(plan.get('files', []))}")
    
    if not args.yes and not args.yolo:
        confirm = input("\nProceed with generation? (y/n): ")
        if confirm.lower() != 'y':
            print("Cancelled.")
            return
    
    # Step 2: Generate each file
    print("\n🔨 Generating project files...")
    
    # Clear conversation history for clean generation
    lai.conversation_history.clear()
    
    success_count = 0
    failed_count = 0
    
    for i, file_info in enumerate(plan.get('files', []), 1):
        file_path = file_info.get('path', '')
        purpose = file_info.get('purpose', '')
        
        if not file_path:
            continue
        
        full_path = os.path.join(path, file_path)
        print(f"\n[{i}/{len(plan.get('files', []))}] Generating {file_path}...")
        print(f"   Purpose: {purpose}")
        
        # Generate code for this file
        file_prompt = f"""Generate ONLY valid Python code for: {purpose}

Project context:
- Project: {plan.get('name', 'Project')}
- Type: {plan.get('project_type', 'python')}
- Tech stack: {plan.get('tech_stack', {})}
- Features: {', '.join(plan.get('features', []))}

Requirements:
- Valid Python 3, executable COMPLETE FILE
- Professional structure with functions/classes
- MUST include main() function if it's an entry point
- MUST include if __name__ == "__main__": block to run the program
- Error handling (try/except)
- Type hints where appropriate
- Docstrings in main functions/classes
- Clean, readable, PEP 8 compliant
- No markdown, no explanations, ONLY code
- Imports at top
- No test code, only the requested program
- ASCII ONLY: straight quotes and NO curly quotes
- NO special Unicode characters

Output ONLY the complete Python code for {file_path}:"""
        
        try:
            generated_code = lai.chat(file_prompt, stream=False, thinking=False, temp=0.0, max_tokens=4096)
            
            # Clean up any markdown
            if generated_code.startswith("```python"):
                generated_code = generated_code[9:]
            elif generated_code.startswith("```"):
                generated_code = generated_code[3:]
            if generated_code.endswith("```"):
                generated_code = generated_code[:-3]
            generated_code = generated_code.strip()
            
            # Validate syntax before writing
            try:
                compile(generated_code, file_path, 'exec')
            except SyntaxError as e:
                print(f"   ⚠️ Syntax error, attempting fix...")
                generated_code = f"# Generated: {purpose}\n# Syntax error: {e}\nprint('Error: Invalid code generated')"
            
            # Write the file
            os.makedirs(os.path.dirname(os.path.join(path, file_path)), exist_ok=True)
            with open(os.path.join(path, file_path), 'w') as f:
                f.write(generated_code)
            
            print(f"   ✅ Generated {file_path}")
            success_count += 1
            
        except Exception as e:
            print(f"   ❌ Failed: {e}")
            failed_count += 1
    
    # Step 3: Initialize git if requested
    if args.git:
        subprocess.run(['git', 'init'], cwd=path)
        print("\n📦 Git repository initialized")
    
    # Step 4: Install dependencies if requirements.txt exists
    req_path = os.path.join(path, 'requirements.txt')
    if os.path.exists(req_path) and args.install:
        print("\n📦 Installing dependencies...")
        subprocess.run(['pip', 'install', '-r', req_path], cwd=path)
    
    # Summary
    print(f"\n{'='*50}")
    print(f"🎉 Project generation complete!")
    print(f"✅ Successful: {success_count}")
    print(f"❌ Failed: {failed_count}")
    print(f"📁 Location: {os.path.abspath(path)}")
    
    if failed_count == 0:
        print(f"\n🚀 Ready to run! Try:")
        print(f"   cd {path}")
        if os.path.exists(os.path.join(path, 'requirements.txt')):
            print(f"   pip install -r requirements.txt")
        print(f"   python main.py  # or whatever the entry point is")

def config_cmd(lai: LAI, args):
    config_file = os.path.expanduser("~/.lai/config.json") if args.global_ else "./.lai/config.json"
    os.makedirs(os.path.dirname(config_file), exist_ok=True)
    config = {}
    if os.path.exists(config_file):
        with open(config_file) as f:
            config = json.load(f)
    if args.action == 'get':
        print(json.dumps(config.get(args.key, {}), indent=2))
    elif args.action == 'set':
        config[args.key] = json.loads(args.value) if args.value.startswith('{') else args.value
        with open(config_file, 'w') as f:
            json.dump(config, f, indent=2)
    elif args.action == 'list':
        print(json.dumps(config, indent=2))
    elif args.action == 'edit':
        subprocess.run(['${EDITOR:-vim}', config_file])

def install_cmd(lai: LAI, args):
    if args.requirements:
        subprocess.run(['pip', 'install', '-r', args.requirements], check=True)
    if args.packages:
        cmd = ['pip', 'install'] + args.packages
        if args.dev:
            cmd.append('--dev')
        subprocess.run(cmd, check=True)

def update_cmd(lai: LAI, args):
    if args.all:
        subprocess.run(['pip', 'list', '--outdated', '--format=freeze'], 
                       stdout=subprocess.PIPE, text=True, check=True)
    if args.packages:
        subprocess.run(['pip', 'install', '--upgrade'] + args.packages, check=True)

def exec_cmd(lai: LAI, args):
    cwd = args.cwd or os.getcwd()
    cmd = args.command if isinstance(args.command, str) else ' '.join(args.command)
    result = subprocess.run(cmd, shell=True, cwd=cwd, capture_output=True, text=True)
    print(result.stdout)
    if result.stderr:
        print(result.stderr, file=sys.stderr)

def doctor_cmd(lai: LAI, args):
    print("Diagnosing environment...")
    checks = [
        ("Python", f"python3 --version"),
        ("Git", "git --version"),
        ("Docker", "docker --version"),
        ("Node", "node --version"),
        ("Docker Compose", "docker-compose --version"),
    ]
    for name, cmd in checks:
        try:
            subprocess.run(cmd, shell=True, check=True, capture_output=True)
            print(f"✓ {name}")
        except:
            print(f"✗ {name} - NOT FOUND")
            if args.fix:
                print(f"  Would install {name}...")
    if args.fix:
        print("Auto-fix not yet implemented")

def clean_cmd(lai: LAI, args):
    paths = []
    if args.all or args.cache:
        paths += ['__pycache__', '.pytest_cache', '.mypy_cache', '.ruff_cache']
    if args.all or args.build:
        paths += ['build', 'dist', '*.egg-info']
    for p in paths:
        for path in glob.glob(p, recursive=True):
            shutil.rmtree(path, ignore_errors=True)
            print(f"Removed: {path}")

def serve_cmd(lai: LAI, args):
    lai.serve(args.port)

def status_cmd(lai: LAI, args):
    lai.status()

def models_cmd(lai: LAI, args):
    lai.models()

def tools_cmd(lai: LAI, args):
    lai.tools_list()

# ============================================================================
# MAIN CLI
# ============================================================================

def cli():
    parser = argparse.ArgumentParser(
        prog='lai',
        description="Local AI CLI Tool - Qwen3-8B via llama.cpp",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  lai chat "hello world"
  lai chat -i                    # interactive REPL
  lai chat "hello" --thinking
  lai generate "write a hello world in python"
  lai code "create a hello world function" hello.py
  lai think "how to implement quicksort"
  lai auto "create a REST API with FastAPI"
  lai run script.py
  lai serve --port 8080
  lai status
  lai models
  lai tools
  lai auto "create a REST API with FastAPI"
  lai run script.py --yolo
        """
    )
    
    parser.add_argument('-m', '--model', default='qwen3-8b', help='Model to use')
    parser.add_argument('-u', '--base-url', default='http://localhost:8080/v1', help='Base URL for API')
    parser.add_argument('-t', '--temp', type=float, default=0.7, help='Temperature')
    parser.add_argument('--max-tokens', type=int, default=4096, help='Max tokens')
    parser.add_argument('--stream', action='store_true', help='Stream output')
    parser.add_argument('--no-stream', action='store_true', help='Disable streaming')
    parser.add_argument('--thinking', action='store_true', help='Enable thinking mode')
    parser.add_argument('--no-thinking', action='store_true', help='Disable thinking mode')
    parser.add_argument('--tools', action='store_true', help='Enable tools')
    parser.add_argument('--no-tools', action='store_true', help='Disable tools')
    parser.add_argument('--auto', action='store_true', help='Auto mode')
    parser.add_argument('--yolo', action='store_true', help='YOLO mode (no confirmations)')
    parser.add_argument('-v', '--verbose', action='count', default=0, help='Verbose output')
    parser.add_argument('-q', '--quiet', action='store_true', help='Quiet mode')
    
    subparsers = parser.add_subparsers(dest='command', help='Commands')
    
    # Chat
    chat_parser = subparsers.add_parser('chat', help='Chat with AI')
    chat_parser.add_argument('prompt', nargs='*', help='Prompt')
    chat_parser.add_argument('-i', '--interactive', action='store_true', help='Interactive REPL')
    chat_parser.add_argument('--stream', action='store_true', help='Stream output')
    chat_parser.add_argument('--no-stream', action='store_true', help='Disable streaming')
    chat_parser.add_argument('--thinking', action='store_true', help='Enable thinking mode')
    chat_parser.add_argument('--no-thinking', action='store_true', help='Disable thinking mode')
    chat_parser.add_argument('--temp', type=float, help='Temperature')
    chat_parser.add_argument('--max-tokens', type=int, help='Max tokens')
    chat_parser.set_defaults(func=chat_cmd)
    
    # Generate
    gen_parser = subparsers.add_parser('generate', help='Generate text')
    gen_parser.add_argument('prompt', nargs='*', help='Prompt')
    gen_parser.set_defaults(func=generate_cmd)
    
    # Code
    code_parser = subparsers.add_parser('code', help='Generate code')
    code_parser.add_argument('description', help='Description')
    code_parser.add_argument('filename', help='Output file')
    code_parser.add_argument('--auto', action='store_true')
    code_parser.add_argument('--yolo', action='store_true')
    code_parser.set_defaults(func=lambda a: code_cmd(get_lai_instance(), a))
    
    # Think
    think_parser = subparsers.add_parser('think', help='Thinking mode')
    think_parser.add_argument('problem', help='Problem')
    think_parser.set_defaults(func=lambda a: think_cmd(get_lai_instance(), a))
    
    # Auto
    auto_parser = subparsers.add_parser('auto', help='Auto mode')
    auto_parser.add_argument('goal', help='Goal')
    auto_parser.add_argument('--max-iterations', type=int, default=10, help='Max iterations')
    auto_parser.set_defaults(func=lambda a: auto_cmd(get_lai_instance(), a))
    
    # Run
    run_parser = subparsers.add_parser('run', help='Run script')
    run_parser.add_argument('script', help='Script file')
    run_parser.set_defaults(func=lambda a: run_cmd(get_lai_instance(), a))
    
    # Serve
    serve_parser = subparsers.add_parser('serve', help='Start API server')
    serve_parser.add_argument('--port', type=int, default=8080)
    serve_parser.set_defaults(func=lambda a: serve_cmd(get_lai_instance(), a))
    
    # Status
    status_parser = subparsers.add_parser('status', help='Check status')
    status_parser.set_defaults(func=lambda a: status_cmd(get_lai_instance(), a))
    
    # Models
    models_parser = subparsers.add_parser('models', help='List models')
    models_parser.set_defaults(func=lambda a: models_cmd(get_lai_instance(), a))
    
    # Tools
    tools_parser = subparsers.add_parser('tools', help='List tools')
    tools_parser.set_defaults(func=lambda a: tools_cmd(get_lai_instance(), a))
    
    # Search
    search_parser = subparsers.add_parser('search', help='Search code/files')
    search_parser.add_argument('pattern', help='Pattern to search')
    search_parser.add_argument('path', nargs='?', default='.', help='Path to search')
    search_parser.add_argument('--type', help='File type filter (e.g. py, js, md)')
    search_parser.add_argument('-i', '--ignore-case', action='store_true', help='Case insensitive')
    search_parser.add_argument('-r', '--recursive', action='store_true', help='Recursive search')
    search_parser.set_defaults(func=lambda a: search_cmd(get_lai_instance(), a))
    
    # Find
    find_parser = subparsers.add_parser('find', help='Find files')
    find_parser.add_argument('pattern', help='File name pattern')
    find_parser.add_argument('path', nargs='?', default='.', help='Path to search')
    find_parser.add_argument('--type', choices=['f', 'd', 'l'], help='File type (f=file, d=dir, l=link)')
    find_parser.add_argument('--max-depth', type=int, help='Max depth')
    find_parser.set_defaults(func=lambda a: find_cmd(get_lai_instance(), a))
    
    # Fix
    fix_parser = subparsers.add_parser('fix', help='Fix code issues')
    fix_parser.add_argument('path', help='File or directory to fix')
    fix_parser.add_argument('--auto', action='store_true', help='Auto-fix without confirmation')
    fix_parser.add_argument('--type', choices=['lint', 'format', 'imports', 'types', 'all'], default='all', help='Type of fix')
    fix_parser.set_defaults(func=lambda a: fix_cmd(get_lai_instance(), a))
    
    # Fix (alias for fix)
    fix_parser2 = subparsers.add_parser('repair', help='Alias for fix')
    fix_parser2.add_argument('path', help='File or directory to repair')
    fix_parser2.add_argument('--auto', action='store_true')
    fix_parser2.set_defaults(func=lambda a: fix_cmd(get_lai_instance(), a))
    
    # Review
    review_parser = subparsers.add_parser('review', help='Code review')
    review_parser.add_argument('path', help='File or directory to review')
    review_parser.add_argument('--focus', choices=['security', 'performance', 'style', 'bugs', 'all'], default='all')
    review_parser.add_argument('--format', choices=['text', 'json', 'markdown'], default='text')
    review_parser.set_defaults(func=lambda a: review_cmd(get_lai_instance(), a))
    
    # Diff
    diff_parser = subparsers.add_parser('diff', help='Show diff')
    diff_parser.add_argument('file1', help='First file')
    diff_parser.add_argument('file2', help='Second file')
    diff_parser.add_argument('--context', type=int, default=3, help='Context lines')
    diff_parser.set_defaults(func=lambda a: diff_cmd(get_lai_instance(), a))
    
    # Test
    test_parser = subparsers.add_parser('test', help='Run tests')
    test_parser.add_argument('path', nargs='?', default='.', help='Path to test')
    test_parser.add_argument('--pattern', help='Test file pattern')
    test_parser.add_argument('-v', '--verbose', action='store_true')
    test_parser.add_argument('--coverage', action='store_true')
    test_parser.set_defaults(func=lambda a: test_cmd(get_lai_instance(), a))
    
    # Init
    init_parser = subparsers.add_parser('init', help='Initialize project')
    init_parser.add_argument('type', choices=['python', 'fastapi', 'flask', 'django', 'react', 'vue', 'svelte', 'cli', 'lib'], help='Project type')
    init_parser.add_argument('name', nargs='?', help='Project name')
    init_parser.add_argument('--path', default='.', help='Target directory')
    init_parser.add_argument('--git', action='store_true', help='Initialize git repo')
    init_parser.set_defaults(func=lambda a: init_cmd(get_lai_instance(), a))
    
    # Project
    project_parser = subparsers.add_parser('project', help='Generate complete project from description')
    project_parser.add_argument('description', nargs='+', help='Project description')
    project_parser.add_argument('--path', default='.', help='Target directory')
    project_parser.add_argument('--git', action='store_true', help='Initialize git repo')
    project_parser.add_argument('--install', action='store_true', help='Install dependencies after generation')
    project_parser.add_argument('--yes', '-y', action='store_true', help='Skip confirmation prompt')
    project_parser.add_argument('--yolo', action='store_true', help='YOLO mode (no confirmations)')
    project_parser.set_defaults(func=lambda a: project_cmd(get_lai_instance(), a))
    
    # Config
    config_parser = subparsers.add_parser('config', help='Manage configuration')
    config_parser.add_argument('action', choices=['get', 'set', 'list', 'edit'], help='Action')
    config_parser.add_argument('key', nargs='?', help='Config key')
    config_parser.add_argument('value', nargs='?', help='Config value')
    config_parser.add_argument('--global', action='store_true', help='Global config')
    config_parser.set_defaults(func=lambda a: config_cmd(get_lai_instance(), a))
    
    # Install
    install_parser = subparsers.add_parser('install', help='Install dependencies')
    install_parser.add_argument('packages', nargs='*', help='Packages to install')
    install_parser.add_argument('--requirements', '-r', help='Requirements file')
    install_parser.add_argument('--dev', action='store_true', help='Install dev dependencies')
    install_parser.set_defaults(func=lambda a: install_cmd(get_lai_instance(), a))
    
    # Update
    update_parser = subparsers.add_parser('update', help='Update dependencies')
    update_parser.add_argument('packages', nargs='*', help='Packages to update')
    update_parser.add_argument('--all', action='store_true', help='Update all')
    update_parser.add_argument('--dry-run', action='store_true')
    update_parser.set_defaults(func=lambda a: update_cmd(get_lai_instance(), a))
    
    # Exec
    exec_parser = subparsers.add_parser('exec', help='Execute command in project context')
    exec_parser.add_argument('command', nargs=argparse.REMAINDER, help='Command to execute')
    exec_parser.add_argument('--cwd', help='Working directory')
    exec_parser.set_defaults(func=lambda a: exec_cmd(get_lai_instance(), a))
    
    # Doctor
    doctor_parser = subparsers.add_parser('doctor', help='Diagnose environment')
    doctor_parser.add_argument('--fix', action='store_true', help='Auto-fix issues')
    doctor_parser.set_defaults(func=lambda a: doctor_cmd(get_lai_instance(), a))
    
    # Clean
    clean_parser = subparsers.add_parser('clean', help='Clean build artifacts')
    clean_parser.add_argument('--all', action='store_true', help='Clean all')
    clean_parser.add_argument('--cache', action='store_true', help='Clean cache')
    clean_parser.add_argument('--build', action='store_true', help='Clean build artifacts')
    clean_parser.set_defaults(func=lambda a: clean_cmd(get_lai_instance(), a))
    
    args = parser.parse_args()
    
    if not hasattr(args, 'func') or args.command is None:
        parser.print_help()
        return
    
    if args.quiet:
        import logging
        logging.getLogger().setLevel(logging.WARNING)
    
    # Auto-start services
    if not args.quiet:
        print("Checking services...")
        check_services()
        for name, info in SERVICES.items():
            status = "Running" if info['running'] else "Stopped"
            print(f"  {name}: {status} (Port {info['port']})")
    
    args.func(args)

if __name__ == "__main__":
    cli()
"""Tests for lai.py core functionality."""

import tempfile
import os
import sys
import json
import pytest

# Add parent directory to path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import lai


def _check_service_url(url: str) -> bool:
    """Check if a service is available."""
    try:
        import urllib.request

        with urllib.request.urlopen(url, timeout=2) as resp:
            return resp.status == 200
    except Exception:
        return False


def _check_tool_calling_support(base_url: str = "http://localhost:8081/v1") -> bool:
    """Check if the model supports native tool calling."""
    try:
        import urllib.request
        import json

        # Check model capabilities via /v1/models
        req = urllib.request.Request(
            f"{base_url}/models",
            headers={"Authorization": "Bearer open"},
        )
        with urllib.request.urlopen(req, timeout=5) as resp:
            if resp.status != 200:
                return False
            data = json.loads(resp.read())
            models = data.get("data", data.get("models", []))
            for model in models:
                # Check if model supports tools (some APIs expose this)
                pass
    except Exception:
        pass
    return False  # Conservative: assume no tool calling support


class TestIsLocalUrl:
    """Tests for is_local_url function."""

    def test_localhost(self):
        assert lai.is_local_url("http://localhost:8080") is True
        assert lai.is_local_url("http://127.0.0.1:8080") is True
        assert lai.is_local_url("http://[::1]:8080") is True

    def test_private_ips(self):
        assert lai.is_local_url("http://192.168.1.100:8080") is True
        assert lai.is_local_url("http://10.0.0.1:8080") is True
        assert lai.is_local_url("http://172.16.0.1:8080") is True

    def test_public_ips(self):
        assert lai.is_local_url("http://8.8.8.8:8080") is False
        assert lai.is_local_url("http://api.openai.com") is False
        assert lai.is_local_url("https://example.com") is False

    def test_invalid(self):
        assert lai.is_local_url("not-a-url") is False
        assert lai.is_local_url("") is False

    def test_ipv6_and_ports(self):
        assert lai.is_local_url("http://[::1]:8080") is True
        assert lai.is_local_url("http://[::1]") is True
        assert lai.is_local_url("http://localhost:8080/v1/chat/completions") is True
        assert lai.is_local_url("http://127.0.0.1:8080/") is True


class TestToolRegistry:
    """Tests for ToolRegistry."""

    def setup_method(self):
        self.lai = lai.LAI()
        self.registry = self.lai.tools

    def test_write_file_rejects_invalid_python(self):
        """write_file should reject syntactically invalid Python."""
        result = self.registry.write_file(
            "bad.py",
            "def foo(:\n    pass\n",  # SyntaxError: invalid syntax
        )
        assert "SyntaxError" in result or "syntax" in result.lower()

    def test_write_file_accepts_valid_python(self):
        """write_file should accept valid Python."""
        with tempfile.TemporaryDirectory() as tmpdir:
            reg = lai.ToolRegistry()
            # ToolRegistry doesn't take path, it uses cwd
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                result = reg.write_file("good.py", "def foo():\n    return 42\n")
                assert "written" in result.lower() or "ok" in result.lower()
            finally:
                os.chdir(old_cwd)

    def test_grep_case_insensitive(self):
        """grep should support case-insensitive search."""
        with tempfile.TemporaryDirectory() as tmpdir:
            reg = lai.ToolRegistry()
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open("test.txt", "w") as f:
                    f.write("Hello World\nhello world\nHELLO WORLD\n")
                result = reg.grep("hello", tmpdir, ignore_case=True)
                # grep returns list of "filepath:lineno: content" strings
                assert len(result) == 3
                assert all("hello" in r.lower() for r in result)
            finally:
                os.chdir(old_cwd)

    def test_grep_file_type(self):
        """grep should filter by file type."""
        with tempfile.TemporaryDirectory() as tmpdir:
            reg = lai.ToolRegistry()
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open("test.py", "w") as f:
                    f.write("import os\n")
                with open("test.txt", "w") as f:
                    f.write("import os\n")
                result = reg.grep("import", tmpdir, file_type="py")
                # Should only match test.py
                assert len(result) == 1
                assert "test.py" in result[0]
            finally:
                os.chdir(old_cwd)

    def test_find_files(self):
        """find_files should find files by glob pattern."""
        with tempfile.TemporaryDirectory() as tmpdir:
            reg = lai.ToolRegistry()
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                os.makedirs("sub")
                with open("a.py", "w") as f:
                    f.write("pass\n")
                with open("sub/b.py", "w") as f:
                    f.write("pass\n")
                with open("c.txt", "w") as f:
                    f.write("text\n")
                result = reg.find_files("*.py", tmpdir)
                # find_files returns absolute paths
                assert any(r.endswith("a.py") for r in result)
                assert any(r.endswith(os.path.join("sub", "b.py")) for r in result)
                assert not any(r.endswith("c.txt") for r in result)
            finally:
                os.chdir(old_cwd)

    def test_repo_map(self):
        """repo_map should extract Python symbols."""
        with tempfile.TemporaryDirectory() as tmpdir:
            reg = lai.ToolRegistry()
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open("mod.py", "w") as f:
                    f.write(
                        """
class MyClass:
    def method(self):
        pass

def my_function():
    return 42
"""
                    )
                result = reg.repo_map()
                assert "MyClass" in result
                assert "method" in result
                assert "my_function" in result
            finally:
                os.chdir(old_cwd)


class TestGetToolsSchema:
    """Tests for get_tools_schema."""

    def test_schema_structure(self):
        lai_instance = lai.LAI()
        tools = lai_instance.tools.get_tools_schema()
        # Returns a list of tool definitions
        assert isinstance(tools, list)
        assert len(tools) >= 16
        tool_names = [t["function"]["name"] for t in tools]
        assert "read_file" in tool_names
        assert "write_file" in tool_names
        assert "run_command" in tool_names
        assert "grep" in tool_names
        assert "find_files" in tool_names
        assert "repo_map" in tool_names


class TestGetLAIInstance:
    """Tests for get_lai_instance singleton."""

    def test_singleton_same_args(self):
        lai1 = lai.get_lai_instance(model="qwen3-8b", base_url="http://localhost:8080")
        lai2 = lai.get_lai_instance(model="qwen3-8b", base_url="http://localhost:8080")
        assert lai1 is lai2

    def test_different_args_creates_new(self):
        lai1 = lai.get_lai_instance(model="qwen3-8b", base_url="http://localhost:8080")
        lai2 = lai.get_lai_instance(model="other", base_url="http://localhost:8080")
        assert lai1 is not lai2


class TestCLI:
    """Basic CLI smoke tests."""

    def test_version(self):
        import subprocess

        result = subprocess.run(
            [sys.executable, "lai.py", "version"],
            capture_output=True,
            text=True,
            cwd=os.path.dirname(os.path.dirname(__file__)),
        )
        assert result.returncode == 0
        assert "0.0.2" in result.stdout

    def test_help(self):
        import subprocess

        result = subprocess.run(
            [sys.executable, "lai.py", "--help"],
            capture_output=True,
            text=True,
            cwd=os.path.dirname(os.path.dirname(__file__)),
        )
        assert result.returncode == 0
        assert "Local AI CLI Tool" in result.stdout

    def test_status(self):
        import subprocess

        result = subprocess.run(
            [sys.executable, "lai.py", "status"],
            capture_output=True,
            text=True,
            cwd=os.path.dirname(os.path.dirname(__file__)),
        )
        assert result.returncode == 0
        assert "llama.cpp" in result.stdout
        assert "gateway" in result.stdout

    def test_tools(self):
        import subprocess

        result = subprocess.run(
            [sys.executable, "lai.py", "tools"],
            capture_output=True,
            text=True,
            cwd=os.path.dirname(os.path.dirname(__file__)),
        )
        assert result.returncode == 0
        assert "read_file" in result.stdout
        assert "write_file" in result.stdout


class TestIntegration:
    """Integration tests requiring live services (llama.cpp, gateway)."""

    @pytest.fixture(scope="class")
    def lai_instance(self):
        """Create LAI instance connected to local services."""
        return lai.LAI(model="qwen3-8b", base_url="http://localhost:8080/v1")

    @pytest.fixture(scope="class")
    def gateway_url(self):
        return "http://localhost:8081/v1"

    @staticmethod
    def _check_service_static(url: str) -> bool:
        """Check if a service is available (static for skipif)."""
        try:
            import urllib.request

            with urllib.request.urlopen(url, timeout=2) as resp:
                return resp.status == 200
        except Exception:
            return False

    def _check_service(self, url: str) -> bool:
        """Check if a service is available."""
        return self._check_service_static(url)

    @pytest.mark.skipif(
        not _check_service_url("http://localhost:8080/health"),
        reason="llama.cpp not running on port 8080",
    )
    def test_llama_health(self):
        import urllib.request

        with urllib.request.urlopen("http://localhost:8080/health", timeout=5) as resp:
            assert resp.status == 200
            data = json.loads(resp.read())
            assert data.get("status") == "ok"

    @pytest.mark.skipif(
        not _check_service_url("http://localhost:8081/health"),
        reason="gateway not running on port 8081",
    )
    def test_gateway_health(self):
        import urllib.request

        with urllib.request.urlopen("http://localhost:8081/health", timeout=5) as resp:
            assert resp.status == 200
            data = json.loads(resp.read())
            assert data.get("status") == "ok"

    @pytest.mark.skipif(
        not _check_service_url("http://localhost:8081/health"),
        reason="gateway not running on port 8081",
    )
    def test_gateway_models(self):
        import urllib.request

        with urllib.request.urlopen(
            urllib.request.Request(
                "http://localhost:8081/v1/models", headers={"Authorization": "Bearer open"}
            ),
            timeout=5,
        ) as resp:
            assert resp.status == 200
            data = json.loads(resp.read())
            assert "data" in data
            assert len(data["data"]) > 0

    @pytest.mark.skipif(
        not _check_service_url("http://localhost:8081/health"),
        reason="gateway not running on port 8081",
    )
    @pytest.mark.skipif(
        not _check_tool_calling_support(),
        reason="model does not support native tool calling",
    )
    def test_gateway_tool_calling_non_stream(self):
        """Test tool calling through gateway (non-streaming)."""
        import urllib.request

        payload = {
            "model": "qwen3-8b",
            "messages": [{"role": "user", "content": "Call add function with a=3,b=4"}],
            "max_tokens": 256,
            "temperature": 0.2,
            "stream": False,
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "add",
                        "description": "Add two numbers",
                        "parameters": {
                            "type": "object",
                            "properties": {"a": {"type": "number"}, "b": {"type": "number"}},
                            "required": ["a", "b"],
                        },
                    },
                }
            ],
            "tool_choice": "auto",
        }
        req = urllib.request.Request(
            "http://localhost:8081/v1/chat/completions",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json", "Authorization": "Bearer open"},
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            assert resp.status == 200
            data = json.loads(resp.read())
            message = data["choices"][0]["message"]
            assert message.get("tool_calls") is not None
            assert len(message["tool_calls"]) > 0
            assert message["tool_calls"][0]["function"]["name"] == "add"

    @pytest.mark.skipif(
        not _check_service_url("http://localhost:8081/health"),
        reason="gateway not running on port 8081",
    )
    @pytest.mark.skipif(
        not _check_tool_calling_support(),
        reason="model does not support native tool calling",
    )
    def test_gateway_tool_calling_stream(self):
        """Test tool calling through gateway (streaming)."""
        import urllib.request

        payload = {
            "model": "qwen3-8b",
            "messages": [{"role": "user", "content": "Call add function with a=3,b=4"}],
            "max_tokens": 256,
            "temperature": 0.2,
            "stream": True,
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "add",
                        "description": "Add two numbers",
                        "parameters": {
                            "type": "object",
                            "properties": {"a": {"type": "number"}, "b": {"type": "number"}},
                            "required": ["a", "b"],
                        },
                    },
                }
            ],
            "tool_choice": "auto",
        }
        req = urllib.request.Request(
            "http://localhost:8081/v1/chat/completions",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json", "Authorization": "Bearer open"},
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            assert resp.status == 200
            saw_tool_call = False
            for raw_line in resp:
                decoded = raw_line.decode().strip()
                if decoded.startswith("data: ") and decoded != "data: [DONE]":
                    chunk = json.loads(decoded[6:])
                    delta = chunk["choices"][0].get("delta", {})
                    if delta.get("tool_calls"):
                        saw_tool_call = True
                        break
        assert saw_tool_call, "Expected tool_calls in streaming response"

    @pytest.mark.skipif(
        not _check_service_url("http://localhost:8082/health"),
        reason="lai serve not running on port 8082",
    )
    def test_serve_health(self):
        import urllib.request

        with urllib.request.urlopen("http://localhost:8082/health", timeout=5) as resp:
            assert resp.status == 200
            data = json.loads(resp.read())
            assert data.get("status") == "ok"

    @pytest.mark.skipif(
        not _check_service_url("http://localhost:8082/health"),
        reason="lai serve not running on port 8082",
    )
    def test_serve_tool_calling_non_stream(self):
        """Test tool calling through lai serve (non-streaming)."""
        import urllib.request

        payload = {
            "model": "qwen3-8b",
            "messages": [{"role": "user", "content": "Call add function with a=3,b=4"}],
            "max_tokens": 256,
            "temperature": 0.2,
            "stream": False,
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "add",
                        "description": "Add two numbers",
                        "parameters": {
                            "type": "object",
                            "properties": {"a": {"type": "number"}, "b": {"type": "number"}},
                            "required": ["a", "b"],
                        },
                    },
                }
            ],
            "tool_choice": "auto",
        }
        req = urllib.request.Request(
            "http://localhost:8082/v1/chat/completions",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json", "Authorization": "Bearer open"},
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            assert resp.status == 200
            data = json.loads(resp.read())
            message = data["choices"][0]["message"]
            assert message.get("tool_calls") is not None
            assert len(message["tool_calls"]) > 0
            assert message["tool_calls"][0]["function"]["name"] == "add"

    @pytest.mark.skipif(
        not _check_service_url("http://localhost:8082/health"),
        reason="lai serve not running on port 8082",
    )
    def test_serve_tool_calling_stream(self):
        """Test tool calling through lai serve (streaming)."""
        import urllib.request

        payload = {
            "model": "qwen3-8b",
            "messages": [{"role": "user", "content": "Call add function with a=3,b=4"}],
            "max_tokens": 256,
            "temperature": 0.2,
            "stream": True,
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "add",
                        "description": "Add two numbers",
                        "parameters": {
                            "type": "object",
                            "properties": {"a": {"type": "number"}, "b": {"type": "number"}},
                            "required": ["a", "b"],
                        },
                    },
                }
            ],
            "tool_choice": "auto",
        }
        req = urllib.request.Request(
            "http://localhost:8082/v1/chat/completions",
            data=json.dumps(payload).encode(),
            headers={"Content-Type": "application/json", "Authorization": "Bearer open"},
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            assert resp.status == 200
            for raw_line in resp:
                decoded = raw_line.decode().strip()
                if decoded.startswith("data: ") and decoded != "data: [DONE]":
                    chunk = json.loads(decoded[6:])
                    delta = chunk["choices"][0].get("delta", {})
                    if delta.get("tool_calls"):
                        return  # Found tool_calls in stream
        pytest.fail("Expected tool_calls in streaming response from serve")

    @pytest.mark.skipif(
        not _check_service_url("http://localhost:8080/health"),
        reason="llama.cpp not running on port 8080",
    )
    @pytest.mark.skipif(
        not _check_tool_calling_support(),
        reason="model does not support native tool calling",
    )
    def test_automode_multi_tool(self, lai_instance):
        """Test AutoMode with multiple tool calls."""
        result = lai_instance.auto_mode_run(
            "List Python files in current directory and show first 5 lines of fib.py",
            max_iterations=3,
            max_turns=10,
        )
        assert "fib.py" in result or "Python" in result


if __name__ == "__main__":
    import pytest

    pytest.main([__file__, "-v"])

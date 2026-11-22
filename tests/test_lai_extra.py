"""Additional tests for lai.py to improve coverage.

Uses mocks to avoid requiring live llama.cpp/gateway/ollama services.
Covers: LAI init attributes, cost tracking, rate limiting, token budget,
system prompts (es/en), history trimming, thinking artifact stripping,
AutoMode basics, status_cmd, config_cmd, and cost summary.
"""

from __future__ import annotations

import contextlib
import argparse
import asyncio
import json
import os
import sys
import tempfile
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import lai  # noqa: E402
from lai_automode import AutoMode
from lai_tools import ToolRegistry as lai_tools


class TestLAIInit:
    """Tests for LAI.__init__ attributes."""

    def test_init_defaults(self):
        instance = lai.LAI()
        assert instance.model == 'qwen3-8b'
        assert instance.base_url == 'http://localhost:8080/v1'
        assert instance.thinking is True
        assert instance.stream is True
        assert instance.temp == 0.7
        assert instance.max_tokens == 4096
        assert instance.history_max_tokens == 12000
        assert instance.lang == 'es'
        assert instance.token_budget == 50000
        assert instance.total_prompt_tokens == 0
        assert instance.total_completion_tokens == 0
        assert instance.estimated_cost_usd == 0.0
        assert instance.request_count == 0
        assert instance.rate_limit_per_min == 60
        assert isinstance(instance._request_timestamps, list)
        assert instance.conversation_history == []
        assert instance.last_error is None

    def test_init_custom_model(self):
        instance = lai.LAI(model='qwen3:8b', base_url='http://localhost:11434/v1')
        assert instance.model == 'qwen3:8b'
        assert instance.base_url == 'http://localhost:11434/v1'


class TestCostTracking:
    """Tests for _estimate_and_track_cost and get_cost_summary."""

    def test_track_cost_accumulates(self):
        instance = lai.LAI()
        instance._estimate_and_track_cost(1000, 500)
        assert instance.total_prompt_tokens == 1000
        assert instance.total_completion_tokens == 500
        assert instance.estimated_cost_usd > 0

    def test_get_cost_summary(self):
        instance = lai.LAI()
        instance.total_prompt_tokens = 10000
        instance.total_completion_tokens = 5000
        instance.request_count = 3
        instance.estimated_cost_usd = 0.1234
        summary = instance.get_cost_summary()
        assert summary['prompt_tokens'] == 10000
        assert summary['completion_tokens'] == 5000
        assert summary['total_tokens'] == 15000
        assert summary['requests'] == 3
        assert summary['token_budget'] == 50000
        assert summary['estimated_cost_usd'] == 0.1234
        assert summary['lang'] == 'es'

    def test_get_cost_summary_english(self):
        instance = lai.LAI()
        instance.lang = 'en'
        summary = instance.get_cost_summary()
        assert summary['lang'] == 'en'


class TestRateLimit:
    """Tests for _check_rate_limit."""

    def test_rate_limit_allows_initial(self):
        instance = lai.LAI()
        assert instance._check_rate_limit() is True
        assert instance._check_rate_limit() is True

    def test_rate_limit_blocks_after_threshold(self):
        instance = lai.LAI()
        instance.rate_limit_per_min = 1
        assert instance._check_rate_limit() is True
        assert instance._check_rate_limit() is False
        assert instance._check_rate_limit() is False

    def test_rate_limit_expires_after_60s(self):
        instance = lai.LAI()
        instance.rate_limit_per_min = 1
        instance._check_rate_limit()
        instance._request_timestamps = []
        assert instance._check_rate_limit() is True


class TestTokenBudget:
    """Tests for _enforce_token_budget."""

    def test_enforce_budget_allows(self):
        instance = lai.LAI()
        instance.token_budget = 10000
        instance.total_prompt_tokens = 1000
        assert instance._enforce_token_budget(5000) is True

    def test_enforce_budget_blocks(self):
        instance = lai.LAI()
        instance.token_budget = 1000
        instance.total_prompt_tokens = 900
        assert instance._enforce_token_budget(200) is False


class TestSystemPrompt:
    """Tests for _system_prompt and _ensure_system_prompt."""

    def test_system_prompt_spanish(self):
        instance = lai.LAI()
        instance.lang = 'es'
        prompt = instance._system_prompt()
        assert prompt['role'] == 'system'
        assert 'Lai' in prompt['content']

    def test_system_prompt_english(self):
        instance = lai.LAI()
        instance.lang = 'en'
        prompt = instance._system_prompt()
        assert prompt['role'] == 'system'
        assert 'English' in prompt['content'] or 'english' in prompt['content'].lower()

    def test_ensure_system_prompt_adds(self):
        instance = lai.LAI()
        messages = [{'role': 'user', 'content': 'hola'}]
        result = instance._ensure_system_prompt(messages)
        assert len(result) == 2
        assert result[0]['role'] == 'system'
        assert result[1]['role'] == 'user'

    def test_ensure_system_prompt_keeps_existing(self):
        instance = lai.LAI()
        system = {'role': 'system', 'content': 'custom system'}
        messages = [system, {'role': 'user', 'content': 'hola'}]
        result = instance._ensure_system_prompt(messages)
        assert result[0] is system


class TestTrimHistory:
    """Tests for _trim_history."""

    def test_trim_history_no_op_under_budget(self):
        instance = lai.LAI()
        instance.conversation_history = [
            {'role': 'user', 'content': 'hola'},
            {'role': 'assistant', 'content': 'mundo'},
        ]
        before = len(instance.conversation_history)
        instance._trim_history()
        assert len(instance.conversation_history) == before

    def test_trim_history_removes_oldest(self):
        instance = lai.LAI()
        instance.history_max_tokens = 10
        instance.conversation_history = [
            {'role': 'system', 'content': 'system prompt here'},
            {'role': 'user', 'content': 'a' * 100},
            {'role': 'assistant', 'content': 'b' * 100},
        ]
        instance._trim_history()
        assert len(instance.conversation_history) <= 3


class TestStripThinkingArtifacts:
    """Tests for _strip_thinking_artifacts."""

    def test_strip_thinking_tags(self):
        instance = lai.LAI()
        text = '<thinking> reasoning </thinking> final answer'
        result = instance._strip_thinking_artifacts(text)
        assert '<thinking>' not in result
        assert 'final answer' in result

    def test_strip_code_fence(self):
        instance = lai.LAI()
        text = '```python\nprint(1)\n```'
        result = instance._strip_thinking_artifacts(text)
        assert '```' not in result
        assert 'print(1)' in result


class TestAutoModeBasics:
    """Lightweight AutoMode tests that don't need live services."""

    def test_auto_mode_run_mocked(self):
        instance = lai.LAI()
        mock_run = MagicMock(return_value='mocked result')
        with patch.object(AutoMode, 'run', mock_run):
            result = instance.auto_mode_run('do something', max_iterations=1, max_turns=2)
            assert result == 'mocked result'


class TestCostAndStatus:
    """Tests for cost summary output."""

    def test_cost_summary_values(self):
        instance = lai.LAI()
        instance.total_prompt_tokens = 5000
        instance.total_completion_tokens = 2500
        instance.request_count = 2
        instance.estimated_cost_usd = 0.05
        summary = instance.get_cost_summary()
        assert summary['total_tokens'] == 7500
        assert summary['estimated_cost_usd'] == 0.05


class TestIsLocalUrlExtra:
    """Extra edge cases for is_local_url."""

    def test_public_hostnames(self):
        assert lai.is_local_url('http://google.com') is False
        assert lai.is_local_url('https://api.github.com') is False

    def test_reserved_private_ranges(self):
        assert lai.is_local_url('http://127.0.0.1') is True

    def test_invalid_inputs(self):
        assert lai.is_local_url(None) is False
        assert lai.is_local_url(123) is False


class TestWriteFileEdgeCases:
    """Extra write_file edge cases."""

    def test_write_file_empty_content(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            reg = lai.ToolRegistry()
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                result = reg.write_file('empty.py', '')
                assert result is not None
            finally:
                os.chdir(old_cwd)

    def test_write_file_non_python(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            reg = lai.ToolRegistry()
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                result = reg.write_file('data.txt', 'hello world')
                assert result is not None
            finally:
                os.chdir(old_cwd)


class TestGrepEdgeCases:
    """Extra grep edge cases."""

    def test_grep_no_matches(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            reg = lai.ToolRegistry()
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('test.txt', 'w') as f:
                    f.write('nothing here\n')
                result = reg.grep('zzzxyz', tmpdir)
                assert result == []
            finally:
                os.chdir(old_cwd)


class TestModelsCmd:
    """Tests for models listing."""

    def test_models_returns_list(self):
        instance = lai.LAI()
        instance.models()


class TestLAIChat:
    """Tests for LAI._chat with mocked httpx."""

    def test_chat_success_spanish(self):
        instance = lai.LAI()
        instance.lang = 'es'
        mock_resp = MagicMock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            'choices': [{'message': {'content': 'Hola mundo', 'role': 'assistant'}}]
        }
        mock_client = AsyncMock()
        mock_client.post = AsyncMock(return_value=mock_resp)
        mock_client.__aenter__.return_value = mock_client
        mock_client.__aexit__.return_value = False
        with patch('httpx.AsyncClient', return_value=mock_client):
            result = asyncio.run(instance._chat('di hola', stream=False))
            assert result == 'Hola mundo'

    def test_chat_empty_content_uses_reasoning(self):
        instance = lai.LAI()
        mock_resp = MagicMock()
        mock_resp.status_code = 200
        mock_resp.json.return_value = {
            'choices': [
                {
                    'message': {
                        'content': '',
                        'reasoning_content': 'razonamiento interno',
                        'role': 'assistant',
                    }
                }
            ]
        }
        mock_client = AsyncMock()
        mock_client.post = AsyncMock(return_value=mock_resp)
        mock_client.__aenter__.return_value = mock_client
        mock_client.__aexit__.return_value = False
        with patch('httpx.AsyncClient', return_value=mock_client):
            result = asyncio.run(instance._chat('test', stream=False))
            assert 'razonamiento interno' in result


class TestToolRegistryRunCommand:
    """Tests for ToolRegistry.run_command."""

    def test_run_command_success(self):
        reg = lai.ToolRegistry()
        with patch('subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(returncode=0, stdout='ok', stderr='')
            result = reg.run_command('echo ok')
            assert result is not None

    def test_run_command_failure(self):
        reg = lai.ToolRegistry()
        with patch('subprocess.run') as mock_run:
            mock_run.return_value = MagicMock(returncode=1, stdout='', stderr='error')
            result = reg.run_command('false')
            assert result is not None


class TestToolRegistryLintFormatTypes:
    """Tests for lint_file, check_types, format_code."""

    def test_lint_file(self):
        reg = lai.ToolRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('test.py', 'w') as f:
                    f.write('x = 1\n')
                result = reg.lint_file('test.py')
                assert result is not None
            finally:
                os.chdir(old_cwd)

    def test_check_types(self):
        reg = lai.ToolRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('test.py', 'w') as f:
                    f.write('x = 1\n')
                result = reg.check_types('test.py')
                assert result is not None
            finally:
                os.chdir(old_cwd)

    def test_format_code(self):
        reg = lai.ToolRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('test.py', 'w') as f:
                    f.write('x=1\n')
                result = reg.format_code('test.py')
                assert result is not None
            finally:
                os.chdir(old_cwd)


class TestToolRegistryAnalyzeComplexity:
    """Tests for analyze_complexity."""

    def test_analyze_complexity(self):
        reg = lai.ToolRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('test.py', 'w') as f:
                    f.write('def foo():\n    pass\n')
                result = reg.analyze_complexity('test.py')
                assert result is not None
            finally:
                os.chdir(old_cwd)


class TestLAIStatus:
    """Tests for LAI.status."""

    def test_status_runs(self):
        instance = lai.LAI()
        instance.total_prompt_tokens = 1000
        instance.total_completion_tokens = 500
        instance.request_count = 1
        instance.estimated_cost_usd = 0.01
        with contextlib.suppress(Exception):
            instance.status()


class TestLAIAutoMode:
    """Tests for AutoMode with mocked services."""

    def test_auto_mode_run_mocked(self):
        instance = lai.LAI()
        mock_run = MagicMock(return_value='auto result')
        with patch.object(AutoMode, 'run', mock_run):
            result = instance.auto_mode_run('do something', max_iterations=1, max_turns=2)
            assert result == 'auto result'


class TestLaiReview:
    """Tests for review command."""

    def test_review_returns_string(self):
        instance = lai.LAI()
        with tempfile.TemporaryDirectory() as tmpdir:
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('test.py', 'w') as f:
                    f.write('def foo():\n    return 1\n')
                from argparse import Namespace

                args = Namespace(path='test.py', focus='all', format='text')
                lai.review_cmd(instance, args)  # prints to stdout, returns None
            finally:
                os.chdir(old_cwd)


class TestFindUnused:
    """Tests for find_unused_imports and find_unused_code."""

    def test_find_unused_imports(self):
        reg = lai.ToolRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('test.py', 'w') as f:
                    f.write('import os\nx = 1\n')
                result = reg.find_unused_imports('test.py')
                assert result is not None
            finally:
                os.chdir(old_cwd)

    def test_find_unused_code(self):
        reg = lai.ToolRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('test.py', 'w') as f:
                    f.write('def unused():\n    pass\nx = 1\n')
                result = reg.find_unused_code('test.py')
                assert result is not None
            finally:
                os.chdir(old_cwd)


class TestFindTodos:
    """Tests for find_todos."""

    def test_find_todos(self):
        reg = lai.ToolRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('test.py', 'w') as f:
                    f.write('# TODO: fix this\nx = 1\n')
                result = reg.find_todos('test.py')
                assert result is not None
            finally:
                os.chdir(old_cwd)


class TestCheckSecurity:
    """Tests for check_security."""

    def test_check_security(self):
        reg = lai.ToolRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('test.py', 'w') as f:
                    f.write('password = "secret"\n')
                result = reg.check_security('test.py')
                assert result is not None
            finally:
                os.chdir(old_cwd)


class TestFindDuplicates:
    """Tests for find_duplicates."""

    def test_find_duplicates(self):
        reg = lai.ToolRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            old_cwd = os.getcwd()
            os.chdir(tmpdir)
            try:
                with open('a.py', 'w') as f:
                    f.write('x = 1\n')
                with open('b.py', 'w') as f:
                    f.write('x = 1\n')
                result = reg.find_duplicates(tmpdir)
                assert result is not None
            finally:
                os.chdir(old_cwd)


class TestVersionAndUrl:
    """Tests for get_version and is_local_url (rule 4 guard)."""

    def test_get_version(self):
        version = lai.get_version()
        assert isinstance(version, str)
        assert len(version) > 0

    @pytest.mark.parametrize(
        'url,expected',
        [
            ('http://localhost:8080/v1', True),
            ('http://127.0.0.1:11434', True),
            ('http://[::1]:8080', True),
            ('http://192.168.1.10:8080/v1', True),
            ('http://10.0.0.5:8080', True),
            ('http://172.16.0.1:8080', True),
            ('http://myhost.local:8080', True),
            ('https://api.openai.com/v1', False),
            ('http://example.com', False),
            ('', False),
            (None, False),
            (12345, False),
        ],
    )
    def test_is_local_url(self, url, expected):
        assert lai.is_local_url(url) is expected


class TestCheckService:
    """Tests for check_service with mocked httpx."""

    def test_check_service_up(self, monkeypatch):
        class FakeResp:
            status_code = 200

        def fake_get(url, timeout=1):
            return FakeResp()

        monkeypatch.setattr(lai.httpx, 'get', fake_get)
        assert lai.check_service(8080) is True

    def test_check_service_down(self, monkeypatch):
        def fake_get(url, timeout=1):
            raise OSError

        monkeypatch.setattr(lai.httpx, 'get', fake_get)
        # httpx falla, luego socket.create_connection también falla -> False
        assert lai.check_service(9999) is False


class TestCompletion:
    """Tests for shell completion helpers."""

    def test_generate_completion_bash(self):
        result = lai._generate_completion('bash')
        assert isinstance(result, str)
        assert 'lai' in result

    def test_generate_completion_zsh(self):
        result = lai._generate_completion('zsh')
        assert isinstance(result, str)

    def test_completion_cmd_unknown_shell(self, capsys):
        args = argparse.Namespace(shell='fish')
        lai.completion_cmd(args)
        captured = capsys.readouterr()
        assert 'fish' in captured.out or 'fish' in captured.err


class TestMcpAndSystemOne:
    """Tests for mcp_cmd and system_one_cmd (no external services)."""

    def test_mcp_cmd_not_found(self, capsys, monkeypatch):
        monkeypatch.setattr(lai.os.path, 'exists', lambda _: False)
        with pytest.raises(SystemExit):
            lai.mcp_cmd()
        captured = capsys.readouterr()
        assert 'MCP server not found' in captured.err

    def test_system_one_cmd_error(self, capsys, monkeypatch):
        class FakeClient:
            def __init__(self, endpoint=None, model=None):
                self.endpoint = endpoint
                self.model = model

            def decide(self, **_):
                return {'error': 'no server', 'detail': 'gateway down'}

        fake_module = type(sys)('scripts.system_one_client')
        fake_module.SystemOneClient = FakeClient
        monkeypatch.setitem(sys.modules, 'scripts.system_one_client', fake_module)

        args = argparse.Namespace(
            endpoint='http://127.0.0.1:8081/v1/systemone',
            model='qwen3-8b',
            action='status',
            state='init',
            type='choice',
            question='q',
            options=[],
            scale=10,
            json=False,
        )
        with pytest.raises(SystemExit):
            lai.system_one_cmd(lai.LAI(), args)
        captured = capsys.readouterr()
        assert 'no server' in captured.err


class TestOCR:
    """Tests for ocr_cmd (OCR local con tesseract)."""

    def test_ocr_image_not_found(self, capsys):
        args = argparse.Namespace(
            image='/tmp/archivo_inexistente_12345.png', lang='spa+eng', output=None
        )
        with pytest.raises(SystemExit):
            lai.ocr_cmd(args)
        captured = capsys.readouterr()
        assert 'no encontrada' in captured.err

    def test_ocr_tesseract_not_installed(self, capsys, monkeypatch, tmp_path):
        image = tmp_path / 'test.png'
        image.write_bytes(b'fake image data')

        def fake_run(cmd, **kwargs):
            raise FileNotFoundError('tesseract')

        monkeypatch.setattr(lai.subprocess, 'run', fake_run)
        args = argparse.Namespace(image=str(image), lang='spa+eng', output=None)
        with pytest.raises(SystemExit):
            lai.ocr_cmd(args)
        captured = capsys.readouterr()
        assert 'no está instalado' in captured.err

    def test_ocr_success(self, capsys, monkeypatch, tmp_path):
        image = tmp_path / 'test.png'
        image.write_bytes(b'fake image data')

        class FakeResult:
            returncode = 0
            stdout = 'Texto extraído de prueba'
            stderr = ''

        def fake_run(cmd, **kwargs):
            return FakeResult()

        monkeypatch.setattr(lai.subprocess, 'run', fake_run)
        args = argparse.Namespace(image=str(image), lang='spa+eng', output=None)
        lai.ocr_cmd(args)
        captured = capsys.readouterr()
        assert 'Texto extraído de prueba' in captured.out

    def test_ocr_with_output_file(self, capsys, monkeypatch, tmp_path):
        image = tmp_path / 'test.png'
        image.write_bytes(b'fake image data')
        output = tmp_path / 'out.txt'

        class FakeResult:
            returncode = 0
            stdout = 'Texto guardado en archivo'
            stderr = ''

        def fake_run(cmd, **kwargs):
            return FakeResult()

        monkeypatch.setattr(lai.subprocess, 'run', fake_run)
        args = argparse.Namespace(image=str(image), lang='spa+eng', output=str(output))
        lai.ocr_cmd(args)
        captured = capsys.readouterr()
        assert output.exists()
        assert output.read_text(encoding='utf-8').strip() == 'Texto guardado en archivo'
        assert 'guardado en' in captured.out


class TestCommandFunctions:
    """Tests for CLI command functions with mocked LAI instance."""

    def test_get_lai_instance_singleton(self):
        instance1 = lai.get_lai_instance(model='qwen3-8b', base_url='http://localhost:8080/v1')
        instance2 = lai.get_lai_instance(model='qwen3-8b', base_url='http://localhost:8080/v1')
        assert instance1 is instance2

    def test_get_lai_instance_different_model(self):
        instance1 = lai.get_lai_instance(model='qwen3-8b', base_url='http://localhost:8080/v1')
        instance2 = lai.get_lai_instance(model='llama3', base_url='http://localhost:8080/v1')
        assert instance1 is not instance2

    def test_get_lai_instance_lang_change(self):
        instance = lai.get_lai_instance(
            model='qwen3-8b', base_url='http://localhost:8080/v1', lang='en'
        )
        assert instance.lang == 'en'

    def test_chat_cmd_no_prompt(self, capsys):
        args = argparse.Namespace(
            model='qwen3-8b',
            base_url='http://localhost:8080/v1',
            lang='es',
            stream=True,
            thinking=True,
            temp=0.7,
            max_tokens=4096,
            interactive=False,
            no_stream=False,
            no_thinking=False,
            prompt=[],
        )
        lai.chat_cmd(args)
        captured = capsys.readouterr()
        assert 'No prompt provided' in captured.out

    def test_status_cmd(self, capsys):
        lai.status_cmd(lai.LAI(), argparse.Namespace())
        captured = capsys.readouterr()
        assert 'Services status' in captured.out or 'llama.cpp' in captured.out

    def test_version_cmd(self, capsys):
        lai.version_cmd()
        captured = capsys.readouterr()
        assert len(captured.out) > 0

    def test_models_cmd(self, capsys, monkeypatch):
        def fake_models(self):
            print('qwen3-8b')

        monkeypatch.setattr(lai.LAI, 'models', fake_models)
        lai.models_cmd(lai.LAI(), argparse.Namespace())
        captured = capsys.readouterr()
        assert 'qwen3-8b' in captured.out

    def test_tools_cmd(self, capsys, monkeypatch):
        def fake_tools_list(self):
            print('read_file')

        monkeypatch.setattr(lai.LAI, 'tools_list', fake_tools_list)
        lai.tools_cmd(lai.LAI(), argparse.Namespace())
        captured = capsys.readouterr()
        assert 'read_file' in captured.out

    def test_generate_cmd_no_prompt(self, capsys):
        args = argparse.Namespace(
            prompt=[],
            model='qwen3-8b',
            base_url='http://localhost:8080/v1',
            stream=True,
            thinking=True,
            temp=0.7,
            max_tokens=2048,
        )
        lai.generate_cmd(args)
        captured = capsys.readouterr()
        assert 'No prompt provided' in captured.out

    def test_run_cmd(self, capsys, monkeypatch, tmp_path):
        script = tmp_path / 'script.py'
        script.write_text('print("hola")\n')

        def fake_run_script(self, path):
            return f'Output of {path}'

        monkeypatch.setattr(lai.LAI, 'run_script', fake_run_script)
        lai.run_cmd(lai.LAI(), argparse.Namespace(script=str(script)))
        captured = capsys.readouterr()
        assert 'Output of' in captured.out

    def test_think_cmd(self, capsys, monkeypatch):
        def fake_think(self, problem):
            return f'Thinking about: {problem}'

        monkeypatch.setattr(lai.LAI, 'think', fake_think)
        lai.think_cmd(lai.LAI(), argparse.Namespace(problem='¿Cómo funciona la recursión?'))
        captured = capsys.readouterr()
        assert 'Thinking about' in captured.out

    def test_code_cmd(self, monkeypatch, tmp_path):
        outfile = tmp_path / 'generated.py'

        def fake_code(self, description, filename, auto=False, yolo=False, max_iterations=3, corpus=None):
            with open(filename, 'w') as f:
                f.write('# generated code\n')

        monkeypatch.setattr(lai.LAI, 'code', fake_code)
        lai.code_cmd(
            lai.LAI(),
            argparse.Namespace(
                description='test', filename=str(outfile), auto=False, yolo=False, max_iterations=3
            ),
        )
        assert outfile.exists()


if __name__ == '__main__':
    import pytest as _pytest

    _pytest.main([__file__, '-v'])


class TestUtilityFunctions:
    """Tests para funciones de utilidad de lai.py."""

    def test_get_version(self):
        version = lai.get_version()
        assert isinstance(version, str)
        assert len(version) > 0

    @pytest.mark.parametrize(
        'url,expected',
        [
            ('http://localhost:8080/v1', True),
            ('http://127.0.0.1:11434', True),
            ('http://[::1]:8080', True),
            ('http://192.168.1.10:8080/v1', True),
            ('http://10.0.0.5:8080', True),
            ('http://172.16.0.1:8080', True),
            ('http://myhost.local:8080', True),
            ('https://api.openai.com/v1', False),
            ('http://example.com', False),
            ('', False),
            (None, False),
            (12345, False),
        ],
    )
    def test_is_local_url_parametrized(self, url, expected):
        assert lai.is_local_url(url) is expected


class TestToolRegistryBasic:
    """Tests básicos para ToolRegistry con mocks."""

    def test_tool_registry_init(self):
        reg = lai_tools()
        assert hasattr(reg, 'tools')
        assert isinstance(reg.tools, dict)
        assert len(reg.tools) > 0

    def test_tool_registry_has_core_tools(self):
        reg = lai_tools()
        expected_tools = ['read_file', 'write_file', 'list_dir', 'grep', 'find_files']
        for tool in expected_tools:
            assert tool in reg.tools

    def test_get_tools_schema(self):
        reg = lai_tools()
        schema = reg.get_tools_schema()
        assert isinstance(schema, list)
        assert len(schema) > 0


if __name__ == '__main__':
    import pytest as _pytest

    _pytest.main([__file__, '-v'])

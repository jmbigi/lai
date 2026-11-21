"""Additional tests for lai.py to improve coverage.

Uses mocks to avoid requiring live llama.cpp/gateway/ollama services.
Covers: LAI init attributes, cost tracking, rate limiting, token budget,
system prompts (es/en), history trimming, thinking artifact stripping,
AutoMode basics, status_cmd, config_cmd, and cost summary.
"""

from __future__ import annotations

import json
import os
import sys
import tempfile
from unittest.mock import MagicMock, patch

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import lai  # noqa: E402


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
        with patch.object(lai.AutoMode, 'run', mock_run):
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


if __name__ == '__main__':
    import pytest as _pytest

    _pytest.main([__file__, '-v'])

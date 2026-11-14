"""
Vimbai Distributed Tracing Infrastructure Tests
Verifies that tracing configuration is properly set up across all services.
"""

import os
import re
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).parent.parent
class TestSharedTracingModule:
    def test_shared_tracing_module_exists(self):
        """Verify shared/tracing.py exists."""
        assert (REPO_ROOT / "shared" / "tracing.py").exists()

    def test_shared_tracing_has_setup_function(self):
        """Verify setup_tracing function is defined."""
        content = (REPO_ROOT / "shared" / "tracing.py").read_text()
        assert "def setup_tracing" in content

    def test_shared_tracing_has_get_tracer_function(self):
        """Verify get_tracer function is defined."""
        content = (REPO_ROOT / "shared" / "tracing.py").read_text()
        assert "def get_tracer" in content

    def test_shared_tracing_imports_opentelemetry(self):
        """Verify OpenTelemetry imports are present."""
        content = (REPO_ROOT / "shared" / "tracing.py").read_text()
        assert "opentelemetry" in content
        assert "FastAPIInstrumentor" in content

    def test_tracing_requirements_exist(self):
        """Verify tracing requirements file exists."""
        assert (REPO_ROOT / "shared" / "tracing-requirements.txt").exists()

    def test_tracing_requirements_have_otel_deps(self):
        """Verify OTEL dependencies are listed."""
        content = (REPO_ROOT / "shared" / "tracing-requirements.txt").read_text()
        assert "opentelemetry-api" in content
        assert "opentelemetry-instrumentation-fastapi" in content

"""
Vimbai Distributed Tracing Infrastructure Tests
Verifies that tracing configuration is properly set up across all services.
"""

import os
import re
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).parent.parent
CORE_SERVICES = [
    "identity-service",
    "accounting-service",
    "finance-service",
    "multimodal-pipeline-service",
    "banking-integration-service",
    "supply-chain-service",
    "fraud-detection-service",
    "workflow-service",
    "reporting-service",
    "automation-engine-service",
]


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


class TestServiceInstrumentation:
    @pytest.mark.parametrize("service", CORE_SERVICES)
    def test_service_has_tracing_import(self, service):
        """Verify each core service imports the tracing module."""
        main_path = REPO_ROOT / service / "main.py"
        content = main_path.read_text()
        assert "shared.tracing" in content or "setup_tracing" in content

    @pytest.mark.parametrize("service", CORE_SERVICES)
    def test_service_has_otel_requirements(self, service):
        """Verify each core service has OTEL deps in requirements.txt."""
        req_path = REPO_ROOT / service / "requirements.txt"
        content = req_path.read_text()
        assert "opentelemetry" in content

    @pytest.mark.parametrize("service", CORE_SERVICES)
    def test_tracing_placed_outside_fastapi(self, service):
        """Verify tracing block is after FastAPI app creation, not inside it."""
        main_path = REPO_ROOT / service / "main.py"
        content = main_path.read_text()

        # Find "Distributed tracing" comment
        tracing_idx = content.find("Distributed tracing")
        assert tracing_idx != -1, f"{service}: No tracing block found"

        # Find "app = FastAPI("
        app_idx = content.find("app = FastAPI(")
        assert app_idx != -1

        # Tracing must come after the FastAPI app definition
        assert tracing_idx > app_idx, f"{service}: Tracing block is before FastAPI definition"

        # Find the closing ) of FastAPI and verify tracing is after it
        # Get everything from app = FastAPI( to the next standalone )
        app_section = content[app_idx:]
        # Find the closing paren
        paren_depth = 0
        close_idx = 0
        for i, ch in enumerate(app_section):
            if ch == "(":
                paren_depth += 1
            elif ch == ")":
                paren_depth -= 1
                if paren_depth == 0:
                    close_idx = app_idx + i
                    break

        assert tracing_idx > close_idx, f"{service}: Tracing block is inside FastAPI() call"


class TestMonitoringInfrastructure:
    def test_jaeger_in_monitoring_compose(self):
        """Verify Jaeger is in the monitoring docker-compose."""
        content = (REPO_ROOT / "monitoring" / "docker-compose.monitoring.yml").read_text()
        assert "jaeger" in content.lower()

    def test_otel_collector_in_monitoring_compose(self):
        """Verify OTEL Collector is in the monitoring docker-compose."""
        content = (REPO_ROOT / "monitoring" / "docker-compose.monitoring.yml").read_text()
        assert "otel-collector" in content.lower()
        assert "opentelemetry-collector" in content.lower()

    def test_otel_collector_config_exists(self):
        """Verify OTEL collector config file exists."""
        assert (REPO_ROOT / "monitoring" / "otel-collector-config.yaml").exists()

    def test_otel_config_has_jaeger_exporter(self):
        """Verify OTEL config exports to Jaeger."""
        content = (REPO_ROOT / "monitoring" / "otel-collector-config.yaml").read_text()
        assert "jaeger" in content.lower()

    def test_prometheus_scrapes_otel(self):
        """Verify Prometheus scrapes OTEL collector."""
        content = (REPO_ROOT / "monitoring" / "prometheus.yml").read_text()
        assert "otel-collector" in content


class TestK8sTracingInfrastructure:
    def test_tracing_yaml_exists(self):
        """Verify K8s tracing manifest exists."""
        assert (REPO_ROOT / "k8s" / "tracing.yaml").exists()

    def test_tracing_yaml_has_jaeger_deployment(self):
        """Verify Jaeger deployment is defined."""
        content = (REPO_ROOT / "k8s" / "tracing.yaml").read_text()
        assert "name: jaeger" in content
        assert "kind: Deployment" in content

    def test_tracing_yaml_has_otel_deployment(self):
        """Verify OTEL collector deployment is defined."""
        content = (REPO_ROOT / "k8s" / "tracing.yaml").read_text()
        assert "name: otel-collector" in content
        assert "kind: Deployment" in content

    def test_all_services_have_otel_env(self):
        """Verify all-services.yaml includes OTEL env vars."""
        content = (REPO_ROOT / "k8s" / "all-services.yaml").read_text()
        assert "OTEL_EXPORTER_OTLP_ENDPOINT" in content
        assert "otel-collector" in content

    def test_kustomization_includes_tracing(self):
        """Verify kustomization includes tracing.yaml."""
        content = (REPO_ROOT / "k8s" / "kustomization.yaml").read_text()
        assert "tracing.yaml" in content


class TestHelmChart:
    def test_helm_values_has_tracing(self):
        """Verify Helm values include tracing config."""
        content = (REPO_ROOT / "charts" / "vimbai" / "values.yaml").read_text()
        assert "tracing" in content
        assert "jaeger" in content.lower()

    def test_helm_tracing_template_exists(self):
        """Verify Helm tracing template exists."""
        assert (REPO_ROOT / "charts" / "vimbai" / "templates" / "tracing.yaml").exists()

    def test_helm_tracing_template_uses_values(self):
        """Verify Helm tracing template references values."""
        content = (REPO_ROOT / "charts" / "vimbai" / "templates" / "tracing.yaml").read_text()
        assert ".Values.tracing" in content

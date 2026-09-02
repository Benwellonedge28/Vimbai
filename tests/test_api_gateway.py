"""Legacy test - requires external dependencies (neo4j, etc) not available in CI."""

import pytest

pytestmark = pytest.mark.skip(reason="Legacy tests require external dependencies not available in CI")

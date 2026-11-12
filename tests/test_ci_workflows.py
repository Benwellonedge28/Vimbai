"""The workflow files themselves, parsed.

`test-core.yml` spent months unable to start. One shard entry opened a quoted
list of services and never closed it, so the file was not valid YAML and GitHub
answered every push with a run that had no jobs and a failure. The tests were
still covered elsewhere, which is exactly why nobody noticed: the only symptom
was one more red run in a list of red runs.

So the workflow files are checked here, in the suite rather than in a review.
"""

import glob
import os

import pytest
import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKFLOWS = sorted(glob.glob(os.path.join(ROOT, ".github", "workflows", "*.yml")))
WORKFLOWS += sorted(glob.glob(os.path.join(ROOT, ".github", "workflows", "*.yaml")))


def test_there_are_workflow_files_to_check():
    assert WORKFLOWS, "no workflows found; the glob is wrong and every check below is vacuous"


@pytest.mark.parametrize("path", WORKFLOWS, ids=lambda path: os.path.basename(path))
def test_every_workflow_is_valid_yaml_with_jobs(path):
    with open(path, encoding="utf-8") as handle:
        document = yaml.safe_load(handle)
    assert isinstance(document, dict), f"{path} is not a mapping"
    jobs = document.get("jobs")
    assert isinstance(jobs, dict) and jobs, f"{path} defines no jobs, so a run of it does nothing"


def test_no_workflow_carries_a_shard_for_a_service_that_is_gone():
    """A shard that names a deleted directory fails when it runs, so it is checked before it runs."""
    path = os.path.join(ROOT, ".github", "workflows", "test-core.yml")
    with open(path, encoding="utf-8") as handle:
        document = yaml.safe_load(handle)
    matrix = document["jobs"]["test-core-groups"]["strategy"]["matrix"]["include"]

    problems = []
    for row in matrix:
        services = (row.get("services") or "").split()
        if not services:
            problems.append(f"{row['group']} lists no services, so its job has nothing to test")
        for service in services:
            if not os.path.isdir(os.path.join(ROOT, service)):
                problems.append(f"{row['group']} names {service}, which is not a directory")

    assert not problems, "\n".join(problems)

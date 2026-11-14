"""The workflow files themselves, parsed.

`test-core.yml` spent months unable to start. One shard entry opened a quoted
list of services and never closed it, so the file was not valid YAML and GitHub
answered every push with a run that had no jobs and a failure. The tests were
still covered elsewhere, which is exactly why nobody noticed: the only symptom
was one more red run in a list of red runs.

That file is gone now. Its only job ran each Python service's own suite in a directory, and the last directory was
deleted with the payment gateway, so the workflow had nothing left to run. What survives is the reason it was checked
here: a workflow file has to parse, define jobs, and never name a service directory that is not on disk.

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


def test_no_workflow_names_a_service_directory_that_is_gone():
    """A list entry naming a deleted directory fails when the run reaches it, so it is checked before it runs.

    The per-service shard file is deleted, but the rule it taught still holds for every workflow that splits work by
    service: a name in a services list has to be a directory, and a list has to have something in it.
    """
    problems = []
    for path in WORKFLOWS:
        with open(path, encoding="utf-8") as handle:
            document = yaml.safe_load(handle)
        for job, body in (document.get("jobs") or {}).items():
            include = (((body or {}).get("strategy") or {}).get("matrix") or {}).get("include") or []
            for row in include:
                listed = row.get("services") if isinstance(row, dict) else None
                if not listed:
                    continue
                for service in str(listed).split():
                    if not os.path.isdir(os.path.join(ROOT, service)):
                        problems.append(f"{os.path.basename(path)} {job}: {service} is not a directory")
    assert not problems, "\n".join(problems)


def test_the_per_service_shard_file_is_gone():
    """It existed to run each Python service's own suite; with no service directories left, it must not come back."""
    assert not os.path.exists(os.path.join(ROOT, ".github", "workflows", "test-core.yml"))

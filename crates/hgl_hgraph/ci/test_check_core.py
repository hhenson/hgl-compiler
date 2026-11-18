"""Check the CI gate's decisions for the outcomes reported by hgl test."""

from subprocess import CompletedProcess
import unittest

from check_core import verify


class CoreGateTest(unittest.TestCase):
    inventory = CompletedProcess([], 2, stdout=(
        "not run  executable  conformance::ready\n"
        "not run  executable  conformance::pending\n"
    ))
    passing = "passed   executable  conformance::ready\n"
    pending = "not run  executable  conformance::pending\n"
    reason = "    fixture.hgl:3:4: the hgraph target does not write a graph switch yet\n"
    rejection_counts = "rejection cases: 0 passed, 0 failed\n"
    expected = {"conformance::pending": "a graph switch"}

    def problems(self, rows, summary, code=2, inventory=None):
        report = CompletedProcess([], code, stdout=rows + self.rejection_counts + summary + "\n")
        return verify(inventory or self.inventory, report, self.expected)

    def test_accepts_only_the_reviewed_case_and_source_located_reason(self):
        summary = "executable cases: 1 passed, 0 failed, 1 not run"
        rows = self.passing + self.pending
        self.assertFalse(self.problems(rows + self.reason, summary))
        for detail in (
            "    the interpreter could not be started\n",
            "    the hgraph target does not write a graph switch yet\n",
            self.reason.replace("a graph switch", "another construct"),
        ):
            with self.subTest(detail=detail):
                self.assertTrue(self.problems(rows + detail, summary))
        self.assertTrue(self.problems(rows + self.reason, summary, code=1))
        self.assertTrue(self.problems(rows + self.reason, summary.replace("1 passed", "2 passed")))

    def test_detects_failed_new_and_missing_cases(self):
        rows = self.passing + self.pending + self.reason
        self.assertTrue(self.problems(
            rows.replace("passed   executable", "FAILED   executable"),
            "executable cases: 0 passed, 1 failed, 1 not run", code=1))
        self.assertTrue(self.problems(
            rows.replace("::pending", "::another"), "executable cases: 1 passed, 0 failed, 1 not run"))
        # Removing the case from the corpus also removes it from discovery.
        reduced = CompletedProcess([], 2, stdout="not run  executable  conformance::ready\n")
        self.assertTrue(self.problems(
            self.passing, "executable cases: 1 passed, 0 failed, 0 not run", code=0, inventory=reduced))
        self.assertTrue(self.problems(
            self.passing + "passed   executable  conformance::pending\n",
            "executable cases: 2 passed, 0 failed, 0 not run", code=0))

    def test_rejected_source_never_admits_a_suite(self):
        rejected = CompletedProcess([], 1, stdout="no case was run\n")
        self.assertTrue(verify(rejected, rejected, self.expected))

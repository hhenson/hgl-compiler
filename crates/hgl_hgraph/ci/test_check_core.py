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

    # A rejection case listed as not yet rejected, as `unsupported_cases.toml`
    # lists one under `[rejections]`.
    rejection = "spec/hgl_core/tests/conformance/rejections/children.hgl:46 fn whole"
    unrejected = {"conformance::rejections/children.hgl::whole"}
    not_rejected = (
        "    expected phase/payload.unguarded on line 49 of spec/hgl_core/tests/conformance/"
        "rejections/children.hgl: not reported\n"
    )

    def rejection_problems(self, row, counts, code, listed=None):
        listed = self.unrejected if listed is None else listed
        rows = row + self.passing + self.pending + self.reason
        summary = counts + "executable cases: 1 passed, 0 failed, 1 not run\n"
        found = CompletedProcess([], code, stdout=rows + summary)
        failed = row.startswith("FAILED")
        discovered = CompletedProcess(
            [], 1 if failed else 2, stdout=row + self.inventory.stdout)
        expected = dict(self.expected)
        expected.update({name: "a whole read needs `all_valid` (TIME-1)" for name in listed})
        return verify(discovered, found, expected, listed)

    def test_a_listed_rejection_fails_only_by_not_being_rejected(self):
        failed = f"FAILED   rejection   {self.rejection}\n"
        counts = "rejection cases: 0 passed, 1 failed\n"
        self.assertFalse(self.rejection_problems(failed + self.not_rejected, counts, code=1))
        # Rejected now, or failing another way, the listed case fails the check.
        passed = f"passed   rejection   {self.rejection}\n"
        self.assertTrue(self.rejection_problems(
            passed, "rejection cases: 1 passed, 0 failed\n", code=2))
        other = "    expected phase/payload.unguarded on line 49: found type/type.mismatch\n"
        self.assertTrue(self.rejection_problems(failed + other, counts, code=1))
        # One that is not listed fails as before, and so does a listed one
        # that is gone.
        self.assertTrue(self.rejection_problems(
            failed + self.not_rejected, counts, code=1, listed=set()))
        self.assertTrue(self.rejection_problems(
            "", "rejection cases: 0 passed, 0 failed\n", code=2))

    def test_a_listed_rejection_of_a_const_fn_is_found_by_its_name(self):
        # A row names the declaration as its source writes it, `const fn
        # climbed`: the name is the last word.
        row = "FAILED   rejection   spec/hgl_core/tests/conformance/rejections/instances.hgl:21 const fn climbed\n"
        detail = (
            "    expected type/generic.instance_limit on line 23 of spec/hgl_core/tests/conformance/"
            "rejections/instances.hgl: not reported\n"
        )
        counts = "rejection cases: 0 passed, 1 failed\n"
        listed = {"conformance::rejections/instances.hgl::climbed"}
        self.assertFalse(self.rejection_problems(row + detail, counts, code=1, listed=listed))

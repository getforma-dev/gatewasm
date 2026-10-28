import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("rotation", Path(__file__).with_name("rotate-management.py"))
rotation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rotation)


class Store:
    def __init__(self):
        self.data = {"initial": {"tenant_id": "ten_synthetic", "management_key": "mgmt_old_synthetic"}}
        self.current = "initial"
        self.pending = None
        self.fail_promote = False

    def versions(self):
        return {version: (["AWSCURRENT"] if version == self.current else []) + (["AWSPENDING"] if version == self.pending else []) for version in self.data}

    def get(self, version=None):
        version = version or self.current
        return version, copy.deepcopy(self.data[version])

    def stage(self, operation, value):
        self.data[operation] = copy.deepcopy(value)
        self.pending = operation

    def promote(self, operation, previous):
        if self.fail_promote:
            self.fail_promote = False
            raise rotation.Failure("injected failure after auth switch")
        assert self.current == previous
        self.current = operation

    def finish(self, operation):
        self.pending = None


class RotationTest(unittest.TestCase):
    def setUp(self):
        self.store = Store()
        self.active = self.store.data["initial"]["management_key"]
        self.fail_switch = False
        self.requests = []

    def request(self, origin, method, path, key, body=None):
        self.requests.append((method, path))  # No credentials in test diagnostics.
        if method == "POST":
            if self.fail_switch:
                self.fail_switch = False
                raise rotation.Failure("injected outage after pending write")
            self.active = body["next_key"]
            return 200, {"rotated": True}
        return (200 if key == self.active else 401), {}

    def run_rotation(self, operation="synthetic-operation"):
        return rotation.rotate(self.store, "https://synthetic.invalid", "plat_synthetic", operation, self.request)

    def test_pending_write_then_outage_is_recoverable(self):
        self.fail_switch = True
        with self.assertRaises(rotation.Failure): self.run_rotation()
        pending = self.store.get("synthetic-operation")[1]["management_key"]
        result = self.run_rotation()
        self.assertEqual(self.active, pending)
        self.assertEqual(result["revoked_key_http"], 401)
        self.assertNotIn(pending, str(result))
        self.assertEqual(self.run_rotation(), result)

    def test_auth_switch_then_secret_promotion_failure_is_recoverable(self):
        self.store.fail_promote = True
        with self.assertRaises(rotation.Failure): self.run_rotation()
        self.assertNotEqual(self.active, self.store.get()[1]["management_key"])
        self.assertEqual(self.run_rotation()["new_key_http"], 200)

    def test_obsolete_retries_and_overlapping_operators_are_refused(self):
        self.fail_switch = True
        with self.assertRaises(rotation.Failure): self.run_rotation()
        with self.assertRaises(rotation.Failure): self.run_rotation("overlapping-operation")
        self.run_rotation()
        self.run_rotation("later-operation")
        with self.assertRaises(rotation.Failure): self.run_rotation()

    def test_completed_operation_cannot_revert_a_later_pending_switch(self):
        self.run_rotation("completed-A")
        self.store.fail_promote = True
        with self.assertRaises(rotation.Failure): self.run_rotation("pending-B")
        active = self.active
        writes = len(self.requests)
        with self.assertRaises(rotation.Failure): self.run_rotation("completed-A")
        self.assertEqual(len(self.requests), writes)
        self.assertEqual(self.active, active)
        self.assertEqual(self.run_rotation("pending-B")["new_key_http"], 200)


if __name__ == "__main__": unittest.main()

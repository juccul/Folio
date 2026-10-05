#!/usr/bin/env python3
"""Exercise rollback and link identity without opening Bluetooth sockets."""
import ctypes
import contextlib
import importlib.util
import io
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('tablet_sniff', Path(__file__).with_name('test-tablet-sniff.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

helper_spec = importlib.util.spec_from_file_location('tablet_helper', Path(__file__).with_name('keep-tablet-active.py'))
helper = importlib.util.module_from_spec(helper_spec)
helper_spec.loader.exec_module(helper)


class FakeController:
    def __init__(self, policy=0x000f):
        self.policy = policy
        self.handle = 7
        self.writes = []
        self.fail_write_once = False

    def connection(self):
        return self.handle

    def read_policy(self, handle):
        if self.handle != handle:
            raise RuntimeError('different connection')
        return self.policy

    def write_policy(self, handle, value):
        if self.handle != handle:
            raise RuntimeError('different connection')
        self.writes.append((handle, value))
        self.policy = value
        if self.fail_write_once:
            self.fail_write_once = False
            raise TimeoutError('controller applied write but response was lost')


class SafetyTests(unittest.TestCase):
    def test_native_connection_layout(self):
        self.assertEqual(ctypes.sizeof(module.ConnectionInfo), 16)
        self.assertEqual(module.ConnectionRequest.info.offset, 8)
        self.assertEqual(ctypes.sizeof(module.ConnectionRequest), 24)
        for value in (0, 1, 7, 0x000f, 0xffff):
            self.assertEqual(module.from_bluetooth(module.to_bluetooth(value)), value)

    def test_only_sniff_bit_changes_and_restores(self):
        controller = FakeController()
        events = []
        with module.without_sniff(controller, 7, lambda kind, **values: events.append(kind)):
            self.assertEqual(controller.policy, 0x000b)
        self.assertEqual(controller.writes, [(7, 0x000b), (7, 0x000f)])
        self.assertEqual(events[-1], 'sniff_restored')

    def test_interrupt_restores(self):
        controller = FakeController()
        with self.assertRaises(KeyboardInterrupt):
            with module.without_sniff(controller, 7, lambda *args, **kwargs: None):
                raise KeyboardInterrupt()
        self.assertEqual(controller.policy, 0x000f)

    def test_failed_reply_after_applied_write_rolls_back(self):
        controller = FakeController()
        controller.fail_write_once = True
        with self.assertRaises(TimeoutError):
            with module.without_sniff(controller, 7, lambda *args, **kwargs: None):
                self.fail('must not enter test body')
        self.assertEqual(controller.policy, 0x000f)

    def test_lost_link_does_not_write_to_new_connection(self):
        for replacement in (None, 8):
            controller = FakeController()
            events = []
            with module.without_sniff(controller, 7, lambda kind, **values: events.append(kind)):
                controller.handle = replacement
            self.assertEqual(controller.writes, [(7, 0x000b)])
            self.assertEqual(events[-1], 'restore_not_needed')

    def test_restoration_preserves_other_policy_changes(self):
        controller = FakeController()
        with module.without_sniff(controller, 7, lambda *args, **kwargs: None):
            controller.policy = 0x0001
        self.assertEqual(controller.policy, 0x0005)

    def test_missing_link_does_not_change_policy(self):
        controller = FakeController()
        controller.handle = None
        with self.assertRaises(RuntimeError):
            with module.without_sniff(controller, 7, lambda *args, **kwargs: None):
                self.fail('must not enter test body')
        self.assertEqual(controller.writes, [])

    def test_already_disabled_does_not_change_policy(self):
        controller = FakeController(policy=1)
        with self.assertRaisesRegex(RuntimeError, 'already disabled'):
            with module.without_sniff(controller, 7, lambda *args, **kwargs: None):
                self.fail('must not enter test body')
        self.assertEqual(controller.writes, [])

    def test_compatibility_helper_can_keep_an_already_disabled_setting(self):
        controller = FakeController(policy=1)
        events = []
        with module.without_sniff(controller, 7, lambda kind, **values: events.append(kind),
                                 allow_already_disabled=True):
            self.assertEqual(controller.policy, 1)
        self.assertEqual(controller.writes, [])
        self.assertIn('sniff_already_disabled', events)

    def test_compatibility_helper_restores_original_disabled_bit_after_external_change(self):
        controller = FakeController(policy=1)
        with module.without_sniff(controller, 7, lambda *args, **kwargs: None,
                                 allow_already_disabled=True):
            controller.policy = 0x0007
        self.assertEqual(controller.policy, 0x0003)


class NativeConnectionSafetyTests(unittest.TestCase):
    def setUp(self):
        self.controller = module.Controller.__new__(module.Controller)
        self.controller.address = bytes.fromhex('E09F2A1EE141')[::-1]
        self.controller.fd = -1  # Never opened; all native ioctls below are mocked.
        self.info = module.ConnectionInfo()
        self.info.address[:] = self.controller.address
        self.info.type = 1
        self.info.state = module.BT_CONNECTED
        self.info.handle = 15

    def ioctl(self, fd, command, buffer, mutate):
        self.assertEqual(command, module.HCIGETCONNINFO)
        request = module.ConnectionRequest.from_buffer_copy(buffer)
        self.assertEqual(bytes(request.address), self.controller.address)
        request.info = self.info
        buffer[:] = bytes(request)
        return 0

    def test_pending_placeholder_from_actual_incident_is_not_a_link(self):
        self.info.state = 6  # BT_CONNECT2
        self.info.handle = 0x0f00  # Exactly the invalid handle seen in the trace.
        with patch.object(module.fcntl, 'ioctl', side_effect=self.ioctl):
            self.assertIsNone(self.controller.connection())
            with self.assertRaisesRegex(RuntimeError, 'reserved or invalid'):
                self.controller.read_policy(0x0f00)
            with self.assertRaisesRegex(RuntimeError, 'reserved or invalid'):
                self.controller.write_policy(0x0f00, 0)
        # There is no .lib here: issuing any native command would fail this test.

    def test_configuration_and_disconnect_states_wait(self):
        for state in (6, 7, 8, 9):
            self.info.state = state
            with patch.object(module.fcntl, 'ioctl', side_effect=self.ioctl):
                self.assertIsNone(self.controller.connection())

    def test_established_link_with_real_handle_is_accepted(self):
        with patch.object(module.fcntl, 'ioctl', side_effect=self.ioctl):
            self.assertEqual(self.controller.connection(), 15)

    def test_reserved_handle_is_rejected_even_if_state_claims_connected(self):
        for handle in (0x0f00, 0xffff):
            self.info.handle = handle
            with patch.object(module.fcntl, 'ioctl', side_effect=self.ioctl):
                self.assertIsNone(self.controller.connection())

    def test_established_link_disappearing_before_command_does_not_issue_command(self):
        with patch.object(module.fcntl, 'ioctl', side_effect=self.ioctl):
            handle = self.controller.connection()
            self.info.state = 8
            with self.assertRaisesRegex(RuntimeError, 'no longer exists'):
                self.controller.read_policy(handle)
            with self.assertRaisesRegex(RuntimeError, 'no longer exists'):
                self.controller.write_policy(handle, 0)

    def test_different_device_is_rejected(self):
        self.info.address[0] ^= 1
        with patch.object(module.fcntl, 'ioctl', side_effect=self.ioctl):
            with self.assertRaisesRegex(RuntimeError, 'different device'):
                self.controller.connection()


class HelperSafetyTests(unittest.TestCase):
    def setUp(self):
        self.now = 0
        self.events = []
        self.controller = FakeController()

    def emit(self, kind, **values):
        self.events.append((kind, values))

    def sleep(self, seconds):
        self.now += seconds

    def simulated_clock(self, sleep=None):
        stack = contextlib.ExitStack()
        stack.enter_context(patch.object(helper.time, 'monotonic', side_effect=lambda: self.now))
        stack.enter_context(patch.object(helper.time, 'sleep', side_effect=sleep or self.sleep))
        stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
        return stack

    def test_duration_expiry_restores_current_link(self):
        with self.simulated_clock():
            helper.guard_link(self.controller, 7, .5, self.emit)
        self.assertEqual(self.controller.writes, [(7, 0x000b), (7, 0x000f)])
        self.assertEqual(self.events[-1][0], 'sniff_restored')

    def test_interrupt_restores_current_link(self):
        def interrupt(seconds):
            raise KeyboardInterrupt()
        with self.simulated_clock(interrupt), self.assertRaises(KeyboardInterrupt):
            helper.guard_link(self.controller, 7, 60, self.emit)
        self.assertEqual(self.controller.policy, 0x000f)

    def test_reconnect_reads_each_new_links_original_policy(self):
        self.controller.handle = None
        phase = 0
        def advance(seconds):
            nonlocal phase
            self.sleep(seconds)
            if phase == 0:
                self.controller.handle = 7
                phase = 1
            elif phase == 1:
                self.controller.handle = None
                phase = 2
            elif phase == 2:
                self.controller.handle = 8
                self.controller.policy = 0x0005
                phase = 3
        with self.simulated_clock(advance):
            helper.run(self.controller, .9, self.emit)
        self.assertEqual(self.controller.writes, [(7, 0x000b), (8, 0x0001), (8, 0x0005)])
        self.assertIn('restore_not_needed', [kind for kind, values in self.events])
        self.assertEqual(self.events[-1], ('sniff_restored', {'handle': 8, 'policy': 5}))

    def test_reasserts_only_sniff_bit_and_preserves_other_policy_changes(self):
        changed = False
        def advance(seconds):
            nonlocal changed
            self.sleep(seconds)
            if self.now >= 1 and not changed:
                self.controller.policy = 0x000d
                changed = True
        with self.simulated_clock(advance):
            helper.guard_link(self.controller, 7, 2.5, self.emit)
        self.assertEqual(self.controller.writes, [(7, 0x000b), (7, 0x0009), (7, 0x000d)])


if __name__ == '__main__':
    unittest.main()

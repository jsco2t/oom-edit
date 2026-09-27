import subprocess
import sys
import termios
import unittest
from unittest.mock import patch

import terminal_guard_pty


class TerminalStateTests(unittest.TestCase):
    def test_exited_child_preserves_master_state_and_detects_unrestored_raw_mode(self):
        result = subprocess.run(
            [sys.executable, terminal_guard_pty.__file__, sys.executable, "-c",
             "import tty; tty.setraw(0)"],
            capture_output=True, text=True, timeout=5,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("GUARD_TERMIOS_RESTORED=False", result.stdout)
        self.assertNotIn("GUARD_PROBE_TIMEOUT", result.stdout)
        self.assertEqual(result.stderr, "")

    def test_exited_child_restoration_is_observed_without_timeout(self):
        result = subprocess.run(
            [sys.executable, terminal_guard_pty.__file__, sys.executable, "-c",
             "import termios,tty; before=termios.tcgetattr(0); tty.setraw(0); "
             "termios.tcsetattr(0,termios.TCSANOW,before)"],
            capture_output=True, text=True, timeout=5,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("GUARD_TERMIOS_RESTORED=True", result.stdout)
        self.assertNotIn("GUARD_PROBE_TIMEOUT", result.stdout)

    def test_darwin_excludes_only_kernel_pending_input_state(self):
        before = [1, 2, 3, termios.ICANON | termios.ECHO, 9600, 9600, [b"x"]]
        after = before.copy()
        after[3] |= termios.PENDIN
        with patch.object(sys, "platform", "darwin"):
            self.assertEqual(terminal_guard_pty.comparable_termios(before),
                             terminal_guard_pty.comparable_termios(after))
            for index in range(len(before)):
                changed = after.copy()
                if index == 6:
                    changed[index] = [b"y"]
                elif index == 3:
                    changed[index] ^= termios.ICANON
                else:
                    changed[index] += 1
                self.assertNotEqual(terminal_guard_pty.comparable_termios(before),
                                    terminal_guard_pty.comparable_termios(changed))
        self.assertTrue(after[3] & termios.PENDIN)

    def test_other_platforms_preserve_every_termios_bit(self):
        state = [1, 2, 3, termios.ICANON | termios.PENDIN, 9600, 9600, [b"x"]]
        with patch.object(sys, "platform", "linux"):
            self.assertEqual(terminal_guard_pty.comparable_termios(state), state)


if __name__ == "__main__":
    unittest.main()

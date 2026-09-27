"""Run a terminal-guard probe under a PTY and verify cooked termios survives."""

import fcntl
import os
import pty
import select
import subprocess
import sys
import termios
import time


def comparable_termios(attributes):
    snapshot = attributes.copy()
    if sys.platform == "darwin":
        # Darwin sets this transient flag when canonical input is restored.
        snapshot[3] &= ~termios.PENDIN
    return snapshot


def main() -> int:
    master, slave = pty.openpty()
    before = termios.tcgetattr(master)

    def attach_terminal() -> None:
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

    child = subprocess.Popen(
        sys.argv[1:],
        stdin=slave,
        stdout=slave,
        stderr=slave,
        preexec_fn=attach_terminal,
    )
    deadline = time.monotonic() + 10
    response_mode = os.environ.get("OOM_GUARD_EMULATE_KEYBOARD")
    recent = b""
    responded = False
    try:
        while True:
            if time.monotonic() > deadline:
                child.kill()
                child.wait()
                print("GUARD_PROBE_TIMEOUT", flush=True)
                return 1
            readable, _, _ = select.select([master], [], [], 0.05)
            chunk = b""
            if readable:
                try:
                    chunk = os.read(master, 65536)
                except OSError:
                    chunk = b""
                if chunk:
                    sys.stdout.buffer.write(chunk)
                    sys.stdout.buffer.flush()
                    recent = (recent + chunk)[-128:]
                    if not responded and b"\x1b[?u\x1b[c" in recent:
                        if response_mode == "supported":
                            os.write(master, b"\x1b[?3u\x1b[?1;2c")
                            responded = True
                        elif response_mode == "unsupported":
                            os.write(master, b"\x1b[?1;2c")
                            responded = True
            if child.poll() is not None and not chunk:
                break
        # The master retains termios after Darwin revokes the exited session's slave.
        after = termios.tcgetattr(master)
        restored = comparable_termios(before) == comparable_termios(after)
        print(f"GUARD_CHILD_EXIT={child.returncode}", flush=True)
        print(f"GUARD_TERMIOS_RESTORED={restored}", flush=True)
        return 0 if restored else 1
    finally:
        os.close(master)
        os.close(slave)


if __name__ == "__main__":
    raise SystemExit(main())

"""Run a terminal-guard probe under a PTY and verify cooked termios survives."""

import fcntl
import os
import pty
import select
import subprocess
import sys
import termios
import time


def main() -> int:
    master, slave = pty.openpty()
    before = termios.tcgetattr(slave)

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
            if child.poll() is not None and not readable:
                break
        after = termios.tcgetattr(slave)
        print(f"GUARD_CHILD_EXIT={child.returncode}", flush=True)
        print(f"GUARD_TERMIOS_RESTORED={before == after}", flush=True)
        return 0 if before == after else 1
    finally:
        os.close(master)
        os.close(slave)


if __name__ == "__main__":
    raise SystemExit(main())

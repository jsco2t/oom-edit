#!/usr/bin/env python3
"""Measure standalone terminal-output timing for sustained keyboard input.

PTY receipt of the synchronized-update terminator is a flush proxy, not a
measurement of when a physical terminal finishes presenting the frame.
"""

from __future__ import annotations

import argparse
import bisect
import fcntl
import gzip
import hashlib
import json
import math
import os
import pty
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time
from pathlib import Path

from acceptance_1mb import FIXTURE_SHA256, fixture_identity, revision_identity, summary

FRAME_END = b"\x1b[?2026l"
KEYS = {"j": b"j", "k": b"k", "down": b"\x1b[B", "up": b"\x1b[A"}
REGIONS = (
    ("area-down", 500, "j"), ("area-up", 1500, "k"),
    ("rust-down", 3000, "j"), ("rust-up", 4000, "k"),
    ("go-down", 20000, "down"), ("go-up", 21000, "up"),
)
EXTRA_REGIONS = (("fresh-down", 0, "down"),)
SOURCE_LINE_BOUNDS = {
    "fresh-down": (0, 10),
    "area-down": (400, 600), "area-up": (1300, 1600),
    "rust-down": (2800, 3100), "rust-up": (3800, 4100),
    "go-down": (19800, 20100), "go-up": (20800, 21100),
}


def poll_timeout(due_ns: int, now_ns: int) -> float:
    return min(0.001, max(0.0, (due_ns - now_ns) / 1e9))


def visible_source_line(output: bytes) -> int | None:
    """Read a visible gutter line without assuming status cells redraw together."""
    labels = re.findall(rb"\x1b\[\d{1,3};2H\s*(\d{1,6})", output)
    return int(labels[-1]) if labels else None


class PtySession:
    def __init__(self, binary: Path, fixture: Path, config_dir: Path,
                 rows: int = 41, cols: int = 100, term: str = "xterm-256color") -> None:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

        def attach_terminal() -> None:
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        env = os.environ.copy()
        env["TERM"] = term
        env["XDG_CONFIG_HOME"] = str(config_dir)
        self.child = subprocess.Popen(
            [str(binary), str(fixture)], stdin=slave, stdout=slave, stderr=slave,
            env=env, preexec_fn=attach_terminal,
        )
        os.close(slave)
        self.master = master
        self.pending = b""
        self.frames: list[tuple[int, int]] = []
        self.output_bytes = 0
        self.output_digest = hashlib.sha256()
        self.key_output_digest = None
        self.output_tail = b""
        self.keyboard_probe_answered = False
        self.opened_ns = time.perf_counter_ns()

    def pump(self, timeout: float) -> bool:
        readable, _, _ = select.select([self.master], [], [], timeout)
        if not readable:
            return False
        try:
            chunk = os.read(self.master, 65536)
        except OSError as error:
            if self.child.poll() is not None:
                return False
            raise RuntimeError(f"standalone PTY closed: {error}") from error
        if not chunk:
            if self.child.poll() is not None:
                return False
            raise RuntimeError("standalone PTY closed before the frame")
        now = time.perf_counter_ns()
        self.output_bytes += len(chunk)
        self.output_digest.update(chunk)
        if self.key_output_digest is not None:
            self.key_output_digest.update(chunk)
        self.output_tail = (self.output_tail + chunk)[-65_536:]
        self.pending += chunk
        if not self.keyboard_probe_answered and b"\x1b[?u\x1b[c" in self.pending:
            os.write(self.master, b"\x1b[?1;2c")
            self.keyboard_probe_answered = True
        while FRAME_END in self.pending:
            position = self.pending.index(FRAME_END) + len(FRAME_END)
            self.frames.append((now, position))
            self.pending = self.pending[position:]
        if len(self.pending) > 128:
            self.pending = self.pending[-128:]
        return True

    def wait_frame(self, after: int, timeout: float = 15.0) -> int:
        deadline = time.monotonic() + timeout
        while len(self.frames) <= after:
            if self.child.poll() is not None:
                raise RuntimeError(f"standalone exited early ({self.child.returncode})")
            if time.monotonic() >= deadline:
                raise TimeoutError("standalone did not flush a complete frame")
            self.pump(min(0.05, deadline - time.monotonic()))
        return self.frames[-1][0]

    def ready(self, rendered_row: int, timeout: float = 15.0) -> tuple[int, int]:
        self.wait_frame(0, timeout=timeout)
        cold_ns = self.frames[0][0] - self.opened_ns
        if rendered_row == 0:
            return cold_ns, self.status_position()[0]
        return cold_ns, self.goto_row(rendered_row, require_status=True)

    def status_position(self) -> tuple[int, int]:
        positions = re.findall(rb"\b([0-9]{1,6}):([0-9]{1,6})\b", self.output_tail)
        if not positions:
            raise ValueError("standalone output omitted the source cursor position")
        return tuple(int(value) for value in positions[-1])

    def goto_row(self, rendered_row: int, *, require_status: bool = False) -> int:
        before = len(self.frames)
        os.write(self.master, f"{rendered_row + 1}gg".encode("ascii"))
        self.wait_frame(before)
        # Keep navigation output out of the measured sequence.
        quiet_until = time.monotonic() + 0.05
        while time.monotonic() < quiet_until:
            self.pump(0.005)
        source_line = visible_source_line(self.output_tail)
        if source_line is not None:
            return source_line
        try:
            return self.status_position()[0]
        except ValueError:
            if require_status:
                raise
            return 0

    def prepare_retained(self) -> None:
        for key in (b"i", b"x", b"\x7f", b"\x1b"):
            before = len(self.frames)
            os.write(self.master, key)
            self.wait_frame(before)

    def serial_keys(self, key: str, count: int, delay_ms: float = 2.0) -> list[dict]:
        records = []
        for index in range(count):
            if index == count // 2:
                time.sleep(0.3)
            before_frames = len(self.frames)
            before_bytes = self.output_bytes
            self.key_output_digest = hashlib.sha256()
            sent_ns = time.perf_counter_ns()
            os.write(self.master, KEYS[key])
            flushed_ns = self.wait_frame(before_frames)
            key_output_sha256 = self.key_output_digest.hexdigest()
            self.key_output_digest = None
            records.append({
                "index": index, "sent_ns": sent_ns, "flush_proxy_ns": flushed_ns,
                "latency_ns": flushed_ns - sent_ns,
                "output_bytes": self.output_bytes - before_bytes,
                "key_output_sha256": key_output_sha256,
                "frames_since_key": len(self.frames) - before_frames,
                "resume": index == count // 2,
            })
            time.sleep(delay_ms / 1000)
        return records

    def burst_keys(self, key: str, count: int,
                   cadence_ms: float = 2.0) -> tuple[list[dict], list[dict]]:
        sent = []
        frame_start = len(self.frames)
        start = time.perf_counter_ns()
        cadence_ns = int(cadence_ms * 1_000_000)
        for index in range(count):
            if index == count // 2:
                time.sleep(0.3)
                start += 300_000_000
            due = start + index * cadence_ns
            while time.perf_counter_ns() < due:
                self.pump(poll_timeout(due, time.perf_counter_ns()))
            timestamp = time.perf_counter_ns()
            os.write(self.master, KEYS[key])
            sent.append({"index": index, "sent_ns": timestamp})
            self.pump(0)
        # A frame emitted after the last send does not mean every queued key ran.
        # The quit command is ordered after the burst and acknowledges its end.
        os.write(self.master, b":q!\r")
        deadline = time.monotonic() + 60.0
        while self.child.poll() is None:
            if time.monotonic() >= deadline:
                raise TimeoutError("standalone did not process the complete key burst")
            self.pump(0.01)
        while self.pump(0):
            pass
        if self.child.returncode != 0:
            raise RuntimeError(f"standalone burst acknowledgment exited {self.child.returncode}")
        frames = []
        prior_ns = 0
        prior_keys_sent = 0
        sent_times = [event["sent_ns"] for event in sent]
        for timestamp, chunk_bytes in self.frames[frame_start:]:
            keys_sent = bisect.bisect_right(sent_times, timestamp)
            scheduled_pause = bool(
                prior_ns and count >= 2 and
                min(timestamp, sent_times[count // 2])
                - max(prior_ns, sent_times[count // 2 - 1]) > 100_000_000
            )
            frames.append({
                "flush_proxy_ns": timestamp, "chunk_bytes": chunk_bytes,
                "keys_sent": keys_sent,
                "gap_ns": timestamp - prior_ns if prior_ns else 0,
                "keys_during_gap": keys_sent - prior_keys_sent,
                "scheduled_pause": scheduled_pause,
            })
            prior_ns = timestamp
            prior_keys_sent = keys_sent
        return sent, frames

    def close(self) -> None:
        try:
            if self.child.poll() is None:
                os.write(self.master, b":q!\r")
                try:
                    self.child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    self.child.send_signal(signal.SIGTERM)
                    try:
                        self.child.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        self.child.kill()
                        self.child.wait(timeout=3)
        finally:
            os.close(self.master)


def verify_trace(serial: list[dict], sent: list[dict], frames: list[dict],
                 count: int, ack_exit_code: int | None) -> None:
    if ack_exit_code != 0:
        raise ValueError("PTY burst did not receive the end-of-queue acknowledgment")
    if len(serial) != count or len(sent) != count or not frames:
        raise ValueError("PTY trace omitted keys or output frames")
    if [row["index"] for row in serial] != list(range(count)):
        raise ValueError("PTY serial key indices are not consecutive")
    if [row["index"] for row in sent] != list(range(count)):
        raise ValueError("PTY burst key indices are not consecutive")
    if any(row["flush_proxy_ns"] < row["sent_ns"]
           or row["latency_ns"] != row["flush_proxy_ns"] - row["sent_ns"]
           or row["output_bytes"] <= 0 or row["frames_since_key"] < 1
           or len(row["key_output_sha256"]) != 64
           for row in serial):
        raise ValueError("PTY serial receipt is missing or inconsistent")
    if any(left["sent_ns"] >= right["sent_ns"] for left, right in zip(sent, sent[1:])):
        raise ValueError("PTY burst input cadence is not monotonic")
    if any(row["keys_sent"] < 0 or row["keys_sent"] > count
           or row["gap_ns"] < 0 or row["keys_during_gap"] < 0 for row in frames):
        raise ValueError("PTY output queue accounting is invalid")
    if any(left["flush_proxy_ns"] > right["flush_proxy_ns"]
           for left, right in zip(frames, frames[1:])):
        raise ValueError("PTY output timestamps are out of order")
    if frames[-1]["keys_sent"] != count:
        raise ValueError("PTY did not flush after the final key")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--role", choices=("candidate", "main"), required=True)
    parser.add_argument("--trials", type=int, default=5)
    parser.add_argument("--keys", type=int, default=1000)
    parser.add_argument("--rows", type=int, default=41)
    parser.add_argument("--cols", type=int, default=100)
    parser.add_argument("--serial-delay-ms", type=float, default=2.0)
    parser.add_argument("--burst-cadence-ms", type=float, default=2.0)
    parser.add_argument("--term", choices=("xterm-256color", "xterm-kitty"),
                        default="xterm-256color")
    parser.add_argument("--post-edit", action="store_true")
    parser.add_argument("--first-frame-timeout-s", type=float, default=15.0)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--region", choices=[region[0] for region in (*REGIONS, *EXTRA_REGIONS)], action="append")
    parser.add_argument("--gate", action="store_true")
    args = parser.parse_args()
    if (min(args.trials, args.keys, args.rows, args.cols) < 1
            or args.serial_delay_ms < 0 or args.burst_cadence_ms <= 0
            or args.first_frame_timeout_s <= 0):
        parser.error("trials, keys, rows, cols, burst cadence and first-frame timeout must be positive")
    if args.gate and (args.trials < 5 or args.keys < 1000 or args.region):
        parser.error("gate requires five trials, 1000 keys and all regions")
    fixture_hash = fixture_identity(args.fixture)
    revision = revision_identity(args.source_root, args.role)
    binary_sha = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    records = []
    failures = []
    regions = REGIONS if not args.region else (*REGIONS, *EXTRA_REGIONS)
    for region, rendered_row, key in regions:
        if args.region and region not in args.region:
            continue
        for trial in range(args.trials):
            with tempfile.TemporaryDirectory(prefix="oom-edit-acceptance-pty-") as config:
                session = PtySession(args.binary.resolve(), args.fixture.resolve(),
                                     Path(config), args.rows, args.cols, args.term)
                try:
                    cold_ns, source_line = session.ready(
                        rendered_row, timeout=args.first_frame_timeout_s)
                    if args.post_edit:
                        session.prepare_retained()
                    low, high = SOURCE_LINE_BOUNDS[region]
                    if not low <= source_line <= high:
                        raise ValueError(f"{region} PTY navigation reached source line {source_line}")
                    serial = session.serial_keys(key, args.keys, args.serial_delay_ms)
                    session.goto_row(rendered_row)
                    sent, frames = session.burst_keys(key, args.keys,
                                                      args.burst_cadence_ms)
                    verify_trace(serial, sent, frames, args.keys,
                                 session.child.returncode)
                    total_bytes = session.output_bytes
                    output_sha256 = session.output_digest.hexdigest()
                finally:
                    session.close()
            common = {
                "role": args.role, "revision": revision,
                "binary_sha256": binary_sha, "fixture_sha256": FIXTURE_SHA256,
                "fixture_hash": fixture_hash, "region": region,
                "rendered_row": rendered_row, "source_line": source_line,
                "key": key, "trial": trial, "cold_ns": cold_ns,
                "rows": args.rows, "cols": args.cols,
                "serial_delay_ms": args.serial_delay_ms,
                "burst_cadence_ms": args.burst_cadence_ms,
                "term": args.term,
                "state": "retained" if args.post_edit else "flat",
                "burst_ack_exit_code": session.child.returncode,
                "total_output_bytes": total_bytes, "output_sha256": output_sha256,
            }
            records.extend({"type": "serial", **common, **item} for item in serial)
            records.extend({"type": "burst-key", **common, **item} for item in sent)
            records.extend({"type": "burst-frame", **common, **item} for item in frames)
            serial_latencies = [item["latency_ns"] for item in serial]
            burst_gaps = [item["gap_ns"] for item in frames
                          if item["gap_ns"] and not item["scheduled_pause"]]
            print(f"{args.role} {region} trial={trial} serial n={len(serial)} "
                  f"{summary(serial_latencies)} burst frames={len(frames)} "
                  f"gap {summary(burst_gaps) if burst_gaps else 'none'} "
                  f"output={total_bytes} bytes", flush=True)
            if args.gate:
                p99 = sorted(serial_latencies)[math.ceil(len(serial_latencies) * 0.99) - 1]
                if p99 >= 25_000_000:
                    failures.append(f"{region}/{trial} serial p99")
                if any(item["gap_ns"] >= 100_000_000
                       and item["keys_during_gap"] > 0
                       and not item["scheduled_pause"] for item in frames):
                    failures.append(f"{region}/{trial} active output pause")
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        opener = gzip.open if args.output.suffix == ".gz" else open
        with opener(args.output, "wt", encoding="utf-8") as destination:
            for row in records:
                destination.write(json.dumps(row, sort_keys=True) + "\n")
    print("PTY flush receipt is a terminal-output proxy, not physical display timing.")
    if failures:
        print("FAIL: " + ", ".join(failures), flush=True)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

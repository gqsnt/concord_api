#!/usr/bin/env python3
r"""One line of TCP socket-pressure counters, for the verification log (diagnostic only).

Usage: python3 -I scripts/socket-snapshot.py LABEL [--details]
       python3 -I scripts/socket-snapshot.py --summary LOG

Prints, for the whole machine (not only this gate's processes):

    socket-snapshot LABEL t=EPOCH tcp=N time_wait=N established=N listen=N other=N
        dyn=START+COUNT excluded=N dyn_in_use=N dyn_free=N active_opens=N

`dyn` is the dynamic (ephemeral) port range that a `bind` to port 0 draws from; `excluded` the
ports of that range reserved by the system (Windows port exclusion ranges, Linux
`ip_local_reserved_ports`); `dyn_in_use` the distinct local ports of that range held by any TCP
socket in any state (TIME_WAIT included); `dyn_free` what remains. `active_opens` is the system's
cumulative count of TCP connections opened (`netstat -s` / `/proc/net/snmp`), so differences
between snapshots measure connection churn. `--details` also prints the raw range and exclusion
listings. Any counter that cannot be read is printed as `?`; the script never fails the gate
(exit 0), because it observes the runner rather than checking the code.

On Windows the counters come from `netstat -ano`, `netstat -s -p tcp` and `netsh`; elsewhere
from `/proc/net/tcp{,6}`, `/proc/net/snmp` and `/proc/sys/net/ipv4`. Platforms without either
source print `?` everywhere.

`--summary LOG` reads the snapshot lines of LOG and prints the peak TIME_WAIT, the smallest
`dyn_free` and the connection churn between the first and the last snapshot.

Why it exists: the 4.3 Windows job 126834420624 failed a loopback `bind("127.0.0.1:0")` with
WSAENOBUFS (10055), which Windows also returns when no dynamic port is free. These counters show,
per gate step, how close the runner was to that limit and how much of it the gate's own tests
used. They establish pressure, not the cause of any particular failure.
"""

import os
import re
import subprocess
import sys
import time

WINDOWS = os.name == "nt"


def run(args):
    try:
        out = subprocess.run(
            args, capture_output=True, timeout=60, check=False
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return None
    return out.decode("utf-8", "replace")


# ---------------------------------------------------------------- parsers (unit-tested) ----


def parse_netstat_connections(text):
    """`netstat -ano` → list of (local_port, state). UDP and header lines are skipped."""
    rows = []
    for line in text.splitlines():
        parts = line.split()
        if len(parts) < 4 or parts[0].upper() != "TCP":
            continue
        local, state = parts[1], parts[3].upper()
        port = local.rsplit(":", 1)[-1]
        if port.isdigit():
            rows.append((int(port), state))
    return rows


def parse_netsh_dynamic(text):
    """`netsh int ipv4 show dynamicport tcp` → (start, count) or None."""
    start = re.search(r"Start Port\s*:\s*(\d+)", text)
    count = re.search(r"Number of Ports\s*:\s*(\d+)", text)
    if start and count:
        return int(start.group(1)), int(count.group(1))
    return None


def parse_netsh_excluded(text):
    """`netsh int ipv4 show excludedportrange protocol=tcp` → list of (start, end)."""
    ranges = []
    for line in text.splitlines():
        m = re.match(r"\s*(\d+)\s+(\d+)\b", line)
        if m:
            ranges.append((int(m.group(1)), int(m.group(2))))
    return ranges


def parse_netstat_active_opens(text):
    """`netstat -s -p tcp` → cumulative active opens, or None."""
    m = re.search(r"Active Opens\s*=\s*(\d+)", text)
    return int(m.group(1)) if m else None


LINUX_STATES = {
    "01": "ESTABLISHED",
    "06": "TIME_WAIT",
    "0A": "LISTENING",
}


def parse_proc_net_tcp(text):
    """`/proc/net/tcp` (or tcp6) → list of (local_port, state)."""
    rows = []
    for line in text.splitlines()[1:]:
        parts = line.split()
        if len(parts) < 4 or ":" not in parts[1]:
            continue
        port = int(parts[1].rsplit(":", 1)[1], 16)
        rows.append((port, LINUX_STATES.get(parts[3].upper(), parts[3].upper())))
    return rows


def parse_reserved_ports(text):
    """`ip_local_reserved_ports` (`8080,9000-9010`) → list of (start, end)."""
    ranges = []
    for item in text.strip().split(","):
        item = item.strip()
        if not item:
            continue
        lo, _, hi = item.partition("-")
        ranges.append((int(lo), int(hi or lo)))
    return ranges


def summarize(rows, dynamic, excluded, active_opens):
    """The counters of one snapshot, as an ordered list of (name, value-or-None)."""
    counts = {"TIME_WAIT": 0, "ESTABLISHED": 0, "LISTENING": 0}
    other = 0
    for _, state in rows or ():
        if state in counts:
            counts[state] += 1
        else:
            other += 1
    fields = [
        ("tcp", len(rows) if rows is not None else None),
        ("time_wait", counts["TIME_WAIT"] if rows is not None else None),
        ("established", counts["ESTABLISHED"] if rows is not None else None),
        ("listen", counts["LISTENING"] if rows is not None else None),
        ("other", other if rows is not None else None),
    ]
    if dynamic is None:
        fields += [("dyn", None), ("excluded", None), ("dyn_in_use", None), ("dyn_free", None)]
    else:
        start, count = dynamic
        end = start + count - 1
        excluded_ports = set()
        for lo, hi in excluded or ():
            excluded_ports.update(range(max(lo, start), min(hi, end) + 1))
        in_use = {p for p, _ in rows or () if start <= p <= end} - excluded_ports
        fields += [
            ("dyn", f"{start}+{count}"),
            ("excluded", len(excluded_ports) if excluded is not None else None),
            ("dyn_in_use", len(in_use) if rows is not None else None),
            (
                "dyn_free",
                count - len(excluded_ports) - len(in_use)
                if rows is not None and excluded is not None
                else None,
            ),
        ]
    fields.append(("active_opens", active_opens))
    return fields


def render(label, fields):
    shown = " ".join(f"{k}={'?' if v is None else v}" for k, v in fields)
    return f"socket-snapshot {label} t={time.time():.1f} {shown}"


# ---------------------------------------------------------------- collection ----


def collect_windows(details):
    text = run(["netstat", "-ano"])
    rows = parse_netstat_connections(text) if text is not None else None
    dyn_text = run(["netsh", "int", "ipv4", "show", "dynamicport", "tcp"]) or ""
    exc_text = run(["netsh", "int", "ipv4", "show", "excludedportrange", "protocol=tcp"])
    stats = run(["netstat", "-s", "-p", "tcp"]) or ""
    dynamic = parse_netsh_dynamic(dyn_text)
    excluded = parse_netsh_excluded(exc_text) if exc_text is not None else None
    extra = [dyn_text, exc_text or ""] if details else []
    return rows, dynamic, excluded, parse_netstat_active_opens(stats), extra


def read(path):
    try:
        with open(path, encoding="ascii", errors="replace") as f:
            return f.read()
    except OSError:
        return None


def collect_linux(details):
    rows = None
    for path in ("/proc/net/tcp", "/proc/net/tcp6"):
        text = read(path)
        if text is not None:
            rows = (rows or []) + parse_proc_net_tcp(text)
    dynamic = None
    port_range = read("/proc/sys/net/ipv4/ip_local_port_range")
    if port_range:
        lo, hi = (int(x) for x in port_range.split()[:2])
        dynamic = (lo, hi - lo + 1)
    reserved = read("/proc/sys/net/ipv4/ip_local_reserved_ports")
    excluded = parse_reserved_ports(reserved) if reserved is not None else None
    active = None
    snmp = read("/proc/net/snmp")
    if snmp:
        tcp = [line.split() for line in snmp.splitlines() if line.startswith("Tcp:")]
        if len(tcp) == 2 and "ActiveOpens" in tcp[0]:
            active = int(tcp[1][tcp[0].index("ActiveOpens")])
    extra = [f"ip_local_port_range: {port_range}", f"ip_local_reserved_ports: {reserved}"]
    return rows, dynamic, excluded, active, extra if details else []


def snapshot(label, details):
    if WINDOWS:
        rows, dynamic, excluded, active, extra = collect_windows(details)
    elif os.path.exists("/proc/net/tcp"):
        rows, dynamic, excluded, active, extra = collect_linux(details)
    else:
        rows, dynamic, excluded, active, extra = None, None, None, None, []
    for block in extra:
        for line in block.strip().splitlines():
            print(f"socket-details {label} | {line}")
    print(render(label, summarize(rows, dynamic, excluded, active)))


def summary(path):
    peak_tw, min_free, first, last = None, None, None, None
    text = read(path) or ""
    for line in text.splitlines():
        if not line.startswith("socket-snapshot "):
            continue
        label = line.split()[1]
        values = dict(kv.split("=", 1) for kv in line.split()[2:] if "=" in kv)
        tw, free, opens = values.get("time_wait"), values.get("dyn_free"), values.get("active_opens")
        if tw and tw.isdigit() and (peak_tw is None or int(tw) > peak_tw[0]):
            peak_tw = (int(tw), label)
        if free and free.lstrip("-").isdigit() and (min_free is None or int(free) < min_free[0]):
            min_free = (int(free), label, values.get("dyn"), values.get("excluded"))
        if opens and opens.isdigit():
            first = first if first is not None else int(opens)
            last = int(opens)
    parts = []
    parts.append(f"peak TIME_WAIT {peak_tw[0]} ({peak_tw[1]})" if peak_tw else "peak TIME_WAIT ?")
    if min_free:
        parts.append(
            f"fewest free dynamic ports {min_free[0]} of {min_free[2]}, "
            f"{min_free[3]} excluded ({min_free[1]})"
        )
    else:
        parts.append("free dynamic ports ?")
    parts.append(f"connections opened {last - first}" if first is not None else "connections opened ?")
    print("socket pressure: " + "; ".join(parts))


def main(argv):
    if len(argv) == 2 and argv[0] == "--summary":
        summary(argv[1])
        return 0
    if len(argv) in (1, 2) and not argv[0].startswith("-") and argv[1:] in ([], ["--details"]):
        snapshot(argv[0], argv[1:] == ["--details"])
        return 0
    print(__doc__.split("\n\n")[1], file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

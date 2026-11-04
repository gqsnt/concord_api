"""Controls of scripts/socket-snapshot.py: its parsers on recorded Windows and Linux output shapes,
the dynamic-range arithmetic, and the summary of a log."""

import contextlib
import importlib.util
import io
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location(
    "socket_snapshot", os.path.join(HERE, "..", "socket-snapshot.py")
)
snap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(snap)

NETSTAT_ANO = """
Active Connections

  Proto  Local Address          Foreign Address        State           PID
  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1012
  TCP    127.0.0.1:49670        127.0.0.1:49671        TIME_WAIT       0
  TCP    127.0.0.1:49671        127.0.0.1:49670        TIME_WAIT       0
  TCP    127.0.0.1:50001        127.0.0.1:50002        ESTABLISHED     4242
  TCP    127.0.0.1:49800        0.0.0.0:0              BOUND           4242
  TCP    [::1]:49700            [::1]:49701            CLOSE_WAIT      4242
  UDP    0.0.0.0:5353           *:*                                    2100
"""

NETSTAT_LOCALIZED = """
Aktive Verbindungen

  Proto  Lokale Adresse         Remoteadresse          Status           PID
  TCP    127.0.0.1:49670        127.0.0.1:49671        WARTEND          0
"""

NETSH_DYNAMIC = """
Protocol tcp Dynamic Port Range
---------------------------------
Start Port      : 49152
Number of Ports : 16384
"""

NETSH_EXCLUDED = """
Protocol tcp Port Exclusion Ranges

Start Port    End Port
----------    --------
      5357        5357
     50000       50059     *

* - Administered port exclusions.
"""

NETSTAT_S = """
TCP Statistics for IPv4

  Active Opens                        = 51234
  Passive Opens                       = 40321
  Failed Connection Attempts          = 17
"""

PROC_NET_TCP = """  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:A5E2 0100007F:A5E3 06 00000000:00000000 03:00000F2C 00000000     0        0 0 3 0000000000000000
   1: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 1 1 0000000000000000
   2: 0100007F:8001 0100007F:1F90 01 00000000:00000000 00:00000000 00000000  1000        0 2 1 0000000000000000
"""


class Parsers(unittest.TestCase):
    def test_windows_connections_keep_ipv4_tcp_rows_including_bound_ones(self):
        rows = snap.parse_netstat_connections(NETSTAT_ANO)
        self.assertEqual(
            rows,
            [
                (135, "LISTENING"),
                (49670, "TIME_WAIT"),
                (49671, "TIME_WAIT"),
                (50001, "ESTABLISHED"),
                (49800, "BOUND"),
            ],
        )

    def test_unrecognized_output_is_unavailable_not_zero(self):
        self.assertIsNone(snap.parse_netstat_connections(NETSTAT_LOCALIZED))
        self.assertIsNone(snap.parse_netstat_connections(""))
        unknown_state = NETSTAT_ANO.replace("ESTABLISHED", "ETABLIE")
        self.assertIsNone(snap.parse_netstat_connections(unknown_state))
        self.assertIsNone(snap.parse_netsh_excluded("Access is denied."))
        self.assertEqual(snap.parse_netsh_excluded("Start Port    End Port\n----------    --------\n"), [])

    def test_windows_range_exclusions_and_opens(self):
        self.assertEqual(snap.parse_netsh_dynamic(NETSH_DYNAMIC), (49152, 16384))
        self.assertEqual(snap.parse_netsh_excluded(NETSH_EXCLUDED), [(5357, 5357), (50000, 50059)])
        self.assertEqual(snap.parse_netstat_active_opens(NETSTAT_S), 51234)
        self.assertIsNone(snap.parse_netsh_dynamic("garbled"))

    def test_linux_proc_states_and_reserved_ports(self):
        rows = snap.parse_proc_net_tcp(PROC_NET_TCP)
        self.assertEqual(rows, [(0xA5E2, "TIME_WAIT"), (0x1F90, "LISTENING"), (0x8001, "ESTABLISHED")])
        self.assertEqual(snap.parse_reserved_ports("8080,9000-9010\n"), [(8080, 8080), (9000, 9010)])
        self.assertEqual(snap.parse_reserved_ports("\n"), [])


class Arithmetic(unittest.TestCase):
    def test_free_ports_exclude_reserved_and_count_each_port_once(self):
        rows = snap.parse_netstat_connections(NETSTAT_ANO)
        fields = dict(
            snap.summarize(rows, (49152, 16384), [(5357, 5357), (50000, 50059)], 7)
        )
        self.assertEqual(fields["time_wait"], 2)
        self.assertEqual(fields["listen"], 1)
        self.assertEqual(fields["excluded"], 60)  # 5357 lies outside the dynamic range
        # 49670, 49671 and the bound-only 49800 are in use; 50001 is inside an exclusion, counted
        # there only.
        self.assertEqual(fields["dyn_in_use"], 3)
        self.assertEqual(fields["dyn_free"], 16384 - 60 - 3)
        self.assertEqual(fields["other"], 1)  # BOUND

    def test_unreadable_sources_print_question_marks(self):
        line = snap.render("x", snap.summarize(None, None, None, None))
        self.assertIn("tcp=? time_wait=?", line)
        self.assertIn("dyn_free=? active_opens=?", line)


class Commands(unittest.TestCase):
    def test_a_failing_command_is_unavailable(self):
        self.assertIsNone(snap.run([sys.executable, "-c", "import sys; print('x'); sys.exit(3)"]))
        self.assertIsNone(snap.run(["definitely-not-a-command-socket-snapshot"]))

    def test_a_timeout_disables_later_snapshots(self):
        saved = (snap.COMMAND_TIMEOUT, os.getcwd())
        with tempfile.TemporaryDirectory() as d:
            os.chdir(d)
            os.mkdir("target")
            try:
                snap.COMMAND_TIMEOUT = 0.2
                with self.assertRaises(snap.Disabled):
                    snap.run([sys.executable, "-c", "import time; time.sleep(5)"])
                self.assertTrue(os.path.exists(snap.DISABLED))
                out = io.StringIO()
                with contextlib.redirect_stdout(out):
                    snap.snapshot("later", False)
                self.assertEqual(out.getvalue().strip(), "socket-snapshot later disabled (an earlier collector timed out)")
            finally:
                snap.COMMAND_TIMEOUT = saved[0]
                os.chdir(saved[1])


class Summary(unittest.TestCase):
    def test_summary_reports_peak_minimum_and_churn(self):
        log = (
            "socket-snapshot a t=1 tcp=3 time_wait=10 established=1 listen=1 other=0 dyn=1+100 excluded=5 dyn_in_use=10 dyn_free=85 active_opens=1000\n"
            "noise\n"
            "socket-snapshot b t=2 tcp=3 time_wait=40 established=1 listen=1 other=0 dyn=1+100 excluded=5 dyn_in_use=60 dyn_free=35 active_opens=1300\n"
            "socket-snapshot c t=3 tcp=3 time_wait=20 established=1 listen=1 other=0 dyn=1+100 excluded=5 dyn_in_use=30 dyn_free=65 active_opens=1350\n"
        )
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "log")
            with open(path, "w", encoding="ascii") as f:
                f.write(log)
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                snap.summary(path)
        self.assertEqual(
            out.getvalue().strip(),
            "socket pressure: peak TIME_WAIT 40 (b); fewest free dynamic ports 35 of 1+100, "
            "5 excluded (b); connections opened 350",
        )


if __name__ == "__main__":
    unittest.main()

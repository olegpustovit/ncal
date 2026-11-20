#!/usr/bin/env python3
"""Compare CLI JSON and exit codes before and after the library extraction."""

import argparse
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
BASELINE = "4ef6569"
USER = {
    "id": "fixture-user",
    "email": "fixture@example.test",
    "accounts": [{"id": "fixture-account", "providerName": "google"}],
}
EVENT = {"id": "fixture-event", "summary": "Fixture event"}
CALENDAR = {"id": "fixture-cal", "accountId": "fixture-account", "provider": "google", "primary": True}
FIXTURES = {
    "getUser": USER,
    "getCalendarLists": [{"calendars": [CALENDAR]}],
    "getColors": {},
    "getEvents": [{"accountId": "fixture-account", "calendarId": "fixture-cal", "events": [EVENT]}],
    "getEvent": {"event": EVENT},
    "createEvent": EVENT,
    "updateEvents": [{"event": EVENT}],
    "deleteEvents": {"success": True},
    "incrementalSync": {"calendars": [CALENDAR], "events": [EVENT]},
    "getContacts": [],
    "getUserPreferences": {"fixture": True},
    "getHolds": {"holds": []},
}
CASES = [
    ("whoami",),
    ("accounts", "list"),
    ("calendars", "list"),
    ("calendars", "colors"),
    ("events", "list", "--refresh", "--all"),
    ("events", "get", "fixture-event"),
    ("events", "create", "--summary", "Fixture event", "--start", "2026-11-20T12:00:00Z",
     "--end", "2026-11-20T13:00:00Z"),
    ("events", "update", "fixture-event", "--summary", "Updated fixture"),
    ("events", "delete", "fixture-event"),
    ("events", "delete", "fixture-event", "--hard"),
    ("sync",),
    ("contacts",),
    ("preferences", "get"),
    ("holds", "list"),
    ("events", "update", "fixture-event"),
    ("events", "list", "--from-time", "invalid-time"),
    ("holds", "get", "missing-hold"),
]


class CalendarFixture(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers["Content-Length"]))
        operation = urlsplit(self.path).path.removeprefix("/v2/")
        if operation not in FIXTURES:
            self.send_error(404)
            return
        body = json.dumps(FIXTURES[operation]).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


def build(root, label, revision):
    source = root / label
    source.mkdir()
    with subprocess.Popen(["git", "archive", revision], cwd=ROOT, stdout=subprocess.PIPE) as archive:
        subprocess.run(["tar", "-x", "-C", source], stdin=archive.stdout, check=True)
        if archive.wait() != 0:
            raise RuntimeError(f"Could not archive {revision}")
    client = source / "crates/ncal-api/src/client.rs"
    code = client.read_text()
    old = 'Url::parse("https://calendar-api.notion.so")?'
    replacement = 'Url::parse(&std::env::var("NCAL_FIXTURE_UPSTREAM").expect("fixture upstream"))?'
    if code.count(old) != 1:
        raise RuntimeError(f"Could not find the fixture client boundary in {revision}")
    client.write_text(code.replace(old, replacement))
    target = ROOT / "target" / "cli-fixture"
    subprocess.run(
        ["cargo", "build", "--locked", "-p", "ncal", "--target-dir", target],
        cwd=source,
        check=True,
    )
    binary = root / f"ncal-{label}"
    shutil.copy2(target / "debug" / "ncal", binary)
    return binary


def run(binary, origin, config, case):
    fixture_env = {
        "NCAL_FIXTURE_UPSTREAM": origin,
        "NCAL_ACCESS_TOKEN": "fixture-access",
        "NCAL_USER_ID": "fixture-user",
        "NCAL_REFRESH_TOKEN": "fixture-refresh",
        "NCAL_TOKEN_EXPIRES_AT": "2099-01-01T00:00:00Z",
    }
    return subprocess.run(
        [binary, "--json", "--config", config, *case],
        env={**os.environ, **fixture_env}, capture_output=True, timeout=5,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", default=BASELINE)
    parser.add_argument("--after", default="HEAD")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="ncal-cli-fixtures-") as directory:
        root = Path(directory)
        before = build(root, "before", args.before)
        after = build(root, "after", args.after)
        with http.server.ThreadingHTTPServer(("127.0.0.1", 0), CalendarFixture) as server:
            thread = threading.Thread(target=server.serve_forever)
            thread.start()
            try:
                origin = f"http://127.0.0.1:{server.server_port}"
                config = root / "config.toml"
                config.write_text(
                    f'[sync]\ntokens_file = "{root / "tokens.json"}"\n'
                    f'[cache]\nfile = "{root / "cache.json"}"\n'
                )
                for case in CASES:
                    baseline = run(before, origin, config, case)
                    current = run(after, origin, config, case)
                    expected = 2 if case in CASES[-3:] else 0
                    assert baseline.returncode == expected, (case, baseline.stderr)
                    assert current.returncode == baseline.returncode, (case, current.stderr)
                    assert current.stdout == baseline.stdout, (case, baseline.stdout, current.stdout)
                    if expected:
                        assert current.stderr.splitlines()[-1] == baseline.stderr.splitlines()[-1], case
            finally:
                server.shutdown()
                thread.join()
    print(f"{len(CASES)} CLI fixtures matched baseline {args.before}")


if __name__ == "__main__":
    main()

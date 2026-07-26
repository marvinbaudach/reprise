#!/usr/bin/env python3
"""Loopback-only ListenBrainz and Last.fm recorder for CUA acceptance tests."""

from __future__ import annotations

import argparse
import hashlib
import json
import threading
import unittest
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs

LISTENBRAINZ_TOKEN = "reprise-e2e-listenbrainz-token"
LASTFM_API_KEY = "reprise-e2e-api-key"
LASTFM_SHARED_SECRET = "reprise-e2e-shared-secret"
LASTFM_SMOKE_API_KEY = "reprise-smoke-api-key"
LASTFM_SMOKE_SHARED_SECRET = "reprise-smoke-shared-secret"
LASTFM_REQUEST_TOKEN = "reprise-e2e-request-token"
LASTFM_SESSION_KEY = "reprise-e2e-session-key"
LASTFM_SMOKE_SESSION_KEY = "reprise-smoke-session-key"
LISTENBRAINZ_USER = "Reprise E2E Listener"
LASTFM_USER = "Reprise E2E Scrobbler"


def _lastfm_signature(
    params: dict[str, str], shared_secret: str = LASTFM_SHARED_SECRET
) -> str:
    material = "".join(
        f"{name}{value}"
        for name, value in sorted(params.items())
        if name not in {"api_sig", "callback", "format"}
    )
    return hashlib.md5(
        f"{material}{shared_secret}".encode(), usedforsecurity=False
    ).hexdigest()


def _first(values: dict[str, list[str]], name: str) -> str:
    return values.get(name, [""])[0]


def _listenbrainz_record(body: dict[str, object]) -> dict[str, object]:
    payload = body.get("payload")
    first_payload = payload[0] if isinstance(payload, list) and payload else {}
    metadata = (
        first_payload.get("track_metadata", {})
        if isinstance(first_payload, dict)
        else {}
    )
    additional = (
        metadata.get("additional_info", {}) if isinstance(metadata, dict) else {}
    )
    return {
        "provider": "listenbrainz",
        "method": "submit-listens",
        "listen_type": body.get("listen_type"),
        "artist": metadata.get("artist_name"),
        "track": metadata.get("track_name"),
        "release": metadata.get("release_name"),
        "duration_ms": additional.get("duration_ms"),
    }


def _lastfm_record(
    params: dict[str, str],
    api_key_valid: bool,
    session_key_valid: bool | None,
    signature_valid: bool,
) -> dict[str, object]:
    method = params.get("method", "")
    suffix = "[0]" if method == "track.scrobble" else ""
    return {
        "provider": "lastfm",
        "method": method,
        "api_key_valid": api_key_valid,
        "session_key_valid": session_key_valid,
        "signature_valid": signature_valid,
        "artist": params.get(f"artist{suffix}"),
        "track": params.get(f"track{suffix}"),
        "album": params.get(f"album{suffix}"),
        "timestamp": params.get(f"timestamp{suffix}"),
    }


class Recorder:
    def __init__(self, log_path: Path) -> None:
        self.log_path = log_path
        self.lock = threading.Lock()

    def append(self, record: dict[str, object]) -> None:
        with self.lock:
            with self.log_path.open("a", encoding="utf-8") as output:
                json.dump(record, output, sort_keys=True)
                output.write("\n")


class ScrobblingHandler(BaseHTTPRequestHandler):
    recorder: Recorder

    def log_message(self, format_string: str, *args: object) -> None:
        return

    def _json_response(self, status: HTTPStatus, body: dict[str, object]) -> None:
        encoded = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def _read_body(self) -> bytes:
        length = int(self.headers.get("Content-Length", "0"))
        return self.rfile.read(length)

    def do_GET(self) -> None:
        if self.path == "/1/validate-token":
            token_valid = self.headers.get("Authorization") in {
                f"Token {LISTENBRAINZ_TOKEN}",
                "Token reprise-smoke-token",
            }
            self.recorder.append(
                {
                    "provider": "listenbrainz",
                    "method": "validate-token",
                    "token_valid": token_valid,
                }
            )
            if token_valid:
                self._json_response(
                    HTTPStatus.OK,
                    {"valid": True, "user_name": LISTENBRAINZ_USER},
                )
            else:
                self._json_response(HTTPStatus.UNAUTHORIZED, {"valid": False})
            return
        if self.path.startswith("/auth/"):
            self._json_response(HTTPStatus.OK, {"authorized": True})
            return
        self._json_response(HTTPStatus.NOT_FOUND, {"error": "not found"})

    def do_POST(self) -> None:
        if self.path == "/1/submit-listens":
            self._handle_listenbrainz_submit()
            return
        if self.path == "/2.0/":
            self._handle_lastfm()
            return
        self._json_response(HTTPStatus.NOT_FOUND, {"error": "not found"})

    def _handle_listenbrainz_submit(self) -> None:
        token_valid = self.headers.get("Authorization") in {
            f"Token {LISTENBRAINZ_TOKEN}",
            "Token reprise-smoke-token",
        }
        try:
            body = json.loads(self._read_body())
        except (json.JSONDecodeError, UnicodeDecodeError):
            self._json_response(HTTPStatus.BAD_REQUEST, {"error": "invalid json"})
            return
        record = _listenbrainz_record(body)
        record["token_valid"] = token_valid
        self.recorder.append(record)
        status = HTTPStatus.OK if token_valid else HTTPStatus.UNAUTHORIZED
        self._json_response(status, {"status": "ok"} if token_valid else {"error": 401})

    def _handle_lastfm(self) -> None:
        values = parse_qs(self._read_body().decode(), keep_blank_values=True)
        params = {name: _first(values, name) for name in values}
        shared_secret = {
            LASTFM_API_KEY: LASTFM_SHARED_SECRET,
            LASTFM_SMOKE_API_KEY: LASTFM_SMOKE_SHARED_SECRET,
        }.get(params.get("api_key", ""))
        api_key_valid = shared_secret is not None
        signature_valid = shared_secret is not None and params.get(
            "api_sig"
        ) == _lastfm_signature(params, shared_secret)
        method = params.get("method")
        session_key_valid = (
            params.get("sk") in {LASTFM_SESSION_KEY, LASTFM_SMOKE_SESSION_KEY}
            if method not in {"auth.getToken", "auth.getSession"}
            else None
        )
        record = _lastfm_record(
            params, api_key_valid, session_key_valid, signature_valid
        )
        self.recorder.append(record)
        if not api_key_valid or not signature_valid:
            self._json_response(HTTPStatus.OK, {"error": 10, "message": "Invalid API key"})
            return

        if method == "auth.getToken":
            self._json_response(HTTPStatus.OK, {"token": LASTFM_REQUEST_TOKEN})
        elif method == "auth.getSession" and params.get("token") == LASTFM_REQUEST_TOKEN:
            self._json_response(
                HTTPStatus.OK,
                {
                    "session": {
                        "name": LASTFM_USER,
                        "key": LASTFM_SESSION_KEY,
                        "subscriber": "0",
                    }
                },
            )
        elif method == "user.getInfo" and session_key_valid:
            self._json_response(HTTPStatus.OK, {"user": {"name": LASTFM_USER}})
        elif method == "track.updateNowPlaying" and session_key_valid:
            self._json_response(HTTPStatus.OK, {"nowplaying": {"ignoredMessage": {}}})
        elif method == "track.scrobble" and session_key_valid:
            self._json_response(HTTPStatus.OK, {"scrobbles": {"accepted": 1}})
        else:
            self._json_response(HTTPStatus.OK, {"error": 9, "message": "Invalid session"})


class ScrobblingApiTests(unittest.TestCase):
    def test_signature_excludes_wire_only_fields(self) -> None:
        params = {
            "method": "auth.getToken",
            "api_key": LASTFM_API_KEY,
            "format": "json",
        }
        signature = _lastfm_signature(params)
        params["api_sig"] = signature
        self.assertEqual(_lastfm_signature(params), signature)
        self.assertEqual(len(signature), 32)

    def test_records_never_contain_provider_secrets(self) -> None:
        params = {
            "method": "track.scrobble",
            "api_key": LASTFM_API_KEY,
            "sk": LASTFM_SESSION_KEY,
            "api_sig": "raw-api-signature-value",
            "artist[0]": "Artist",
            "track[0]": "Track",
        }
        encoded = json.dumps(_lastfm_record(params, True, True, True))
        self.assertNotIn(LASTFM_SHARED_SECRET, encoded)
        self.assertNotIn(LASTFM_SESSION_KEY, encoded)
        self.assertNotIn("raw-api-signature-value", encoded)
        self.assertIn('"signature_valid": true', encoded)

    def test_listenbrainz_record_keeps_only_assertable_metadata(self) -> None:
        record = _listenbrainz_record(
            {
                "listen_type": "single",
                "payload": [
                    {
                        "track_metadata": {
                            "artist_name": "Artist",
                            "track_name": "Track",
                            "release_name": "Album",
                            "additional_info": {"duration_ms": 120000},
                        }
                    }
                ],
            }
        )
        self.assertEqual(record["track"], "Track")
        self.assertEqual(record["duration_ms"], 120000)


def serve(port_file: Path, log_file: Path) -> None:
    log_file.parent.mkdir(parents=True, exist_ok=True)
    log_file.write_text("", encoding="utf-8")
    recorder = Recorder(log_file)

    class BoundHandler(ScrobblingHandler):
        pass

    BoundHandler.recorder = recorder
    server = ThreadingHTTPServer(("127.0.0.1", 0), BoundHandler)
    port_file.write_text(f"{server.server_port}\n", encoding="utf-8")
    server.serve_forever()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port-file", type=Path)
    parser.add_argument("--log-file", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        unittest.main(argv=[__file__])
        return
    if args.port_file is None or args.log_file is None:
        parser.error("--port-file and --log-file are required")
    serve(args.port_file, args.log_file)


if __name__ == "__main__":
    main()

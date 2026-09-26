"""Tiny Wikidata Query Service client with an on-disk response cache.

Every query result is cached (gzip JSON, keyed by sha1 of the query text) under
tools/lexicon_build/cache/wikidata/sparql/, so a rerun with the same config is fully offline.
Respects 429 Retry-After, backs off on 5xx, and reports timeouts to the caller (which splits work).
"""

from __future__ import annotations

import gzip
import hashlib
import json
import os
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
CACHE = HERE.parent / "cache" / "wikidata"
SPARQL_CACHE = CACHE / "sparql"
ENDPOINT = "https://query.wikidata.org/sparql"
USER_AGENT = (
    "BrightestDayLexiconBuild/0.1 (offline game-lexicon build of named entities; results cached, "
    "serial low-rate queries) Python-urllib"
)
OFFLINE = os.environ.get("BD_WIKIDATA_OFFLINE") == "1"

_lock = threading.Lock()
_stats = {"hit": 0, "miss": 0, "timeouts": 0, "retries": 0, "secs": 0.0}


class QueryTimeout(Exception):
    pass


def log(*a):
    print(*a, file=sys.stderr, flush=True)


def _key(q: str) -> Path:
    h = hashlib.sha1(q.encode("utf-8")).hexdigest()
    return SPARQL_CACHE / h[:2] / f"{h}.json.gz"


def cached(q: str) -> bool:
    return _key(q).exists()


def query(q: str, *, timeout_ok: bool = False, tag: str = "") -> list[dict]:
    """Run a SELECT query; returns list of {var: value-string}. Raises QueryTimeout on server timeout."""
    p = _key(q)
    if p.exists():
        with gzip.open(p, "rt", encoding="utf-8") as f:
            data = json.load(f)
        with _lock:
            _stats["hit"] += 1
        if data.get("__timeout__"):
            raise QueryTimeout(tag)
        return data["rows"]
    if OFFLINE:
        raise RuntimeError(f"offline mode and query not cached: {tag}")
    body = urllib.parse.urlencode({"query": q}).encode()
    attempt = 0
    while True:
        attempt += 1
        req = urllib.request.Request(
            ENDPOINT,
            data=body,
            headers={
                "User-Agent": USER_AGENT,
                "Accept": "application/sparql-results+json",
                "Content-Type": "application/x-www-form-urlencoded",
            },
        )
        t = time.time()
        try:
            with urllib.request.urlopen(req, timeout=90) as r:
                raw = r.read()
            dt = time.time() - t
            res = json.loads(raw)
            rows = [
                {k: v["value"] for k, v in b.items()} for b in res["results"]["bindings"]
            ]
            _save(p, {"rows": rows})
            with _lock:
                _stats["miss"] += 1
                _stats["secs"] += dt
            return rows
        except urllib.error.HTTPError as e:
            txt = ""
            try:
                txt = e.read().decode("utf-8", "replace")[:2000]
            except Exception:
                pass
            if e.code == 429:
                wait = int(e.headers.get("Retry-After", "30") or 30)
                log(f"  429 throttled ({tag}); sleeping {wait}s")
                time.sleep(min(wait, 120))
                continue
            if "TimeoutException" in txt or "timeout" in txt.lower() and e.code == 500:
                with _lock:
                    _stats["timeouts"] += 1
                if timeout_ok:
                    _save(p, {"__timeout__": True, "rows": []})
                raise QueryTimeout(tag)
            if e.code in (500, 502, 503, 504) and attempt < 6:
                with _lock:
                    _stats["retries"] += 1
                log(f"  HTTP {e.code} ({tag}) attempt {attempt}; backing off")
                time.sleep(5 * attempt)
                continue
            raise RuntimeError(f"HTTP {e.code} for {tag}: {txt[:500]}")
        except (urllib.error.URLError, TimeoutError, ConnectionError, json.JSONDecodeError) as e:
            if isinstance(e, TimeoutError) or "timed out" in str(e):
                with _lock:
                    _stats["timeouts"] += 1
                if attempt >= 2:
                    raise QueryTimeout(tag)
            if attempt < 6:
                with _lock:
                    _stats["retries"] += 1
                log(f"  network error ({tag}) {e!r}; retry {attempt}")
                time.sleep(5 * attempt)
                continue
            raise


def _save(p: Path, obj):
    p.parent.mkdir(parents=True, exist_ok=True)
    tmp = p.with_suffix(".part")
    with gzip.open(tmp, "wt", encoding="utf-8", compresslevel=6) as f:
        json.dump(obj, f, ensure_ascii=False, separators=(",", ":"))
    tmp.replace(p)


def stats() -> dict:
    return dict(_stats)


def qid(uri: str) -> str:
    return uri.rsplit("/", 1)[-1]

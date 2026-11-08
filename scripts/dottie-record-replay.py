#!/usr/bin/env python3
"""Re-record the offline replay fixture for apps/dottie/tests/replay.rs.

Runs the replay plan live in one process (about 40 GMGN weight, 5 Fomo calls
and a few Jupiter quotes; read-only), then keeps only the fields Dottie reads
so the fixture stays small. Keys come from the environment, never the repo:

  set -a; . ~/.config/hoodit/gmgn.env; . ~/.config/dottie/fomo.env; set +a
  scripts/dottie-record-replay.py

GMGN's key is shared with Hoodit's bot: run this once, not in a loop.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "apps" / "dottie"
PLAN = APP / "tests" / "fixtures" / "replay-plan.json"
OUT = APP / "tests" / "fixtures" / "replay.json"
SECRETS = ("GMGN_API_KEY", "FOMO_API_KEY", "JUPITER_API_KEY", "SOLANA_RPC_URL")


def gmgn_fields() -> set[str]:
    src = (APP / "src" / "providers" / "gmgn" / "types.rs").read_text()
    fields = set(re.findall(r"pub (\w+):", src)) | set(re.findall(r'alias = "(\w+)"', src))
    # Envelopes and list keys around the typed rows.
    return fields | {"data", "rank", "list", "coins", "activities", "tokens", "code", "next",
                     "new_creation", "near_completion", "completed", "$fail", "message", "source"}


def slim(value, fields):
    if isinstance(value, dict):
        return {k: slim(v, fields) for k, v in value.items() if k in fields}
    if isinstance(value, list):
        # Long feeds and boards: the first rows carry the shape.
        return [slim(v, fields) for v in value[:25]]
    return value


RPC_FIELDS = {"value", "account", "data", "parsed", "info", "mint", "tokenAmount", "amount",
              "decimals", "owner", "type", "mintAuthority", "freezeAuthority", "extensions",
              "extension", "state", "newerTransferFee", "transferFeeBasisPoints", "$fail",
              "code", "message", "source"}
FOMO_DROP = {"avatar", "imageUrl", "image", "tokenImage", "icon", "clan", "logo"}


def drop(value, keys):
    if isinstance(value, dict):
        return {k: drop(v, keys) for k, v in value.items() if k not in keys}
    if isinstance(value, list):
        return [drop(v, keys) for v in value]
    return value


def main() -> None:
    started = int(time.time())
    with tempfile.TemporaryDirectory() as tmp:
        recorded = Path(tmp) / "recorded.json"
        subprocess.run(
            ["cargo", "run", "-q", "-p", "dottie", "--example", "read", "--", "--plan", str(PLAN)],
            cwd=ROOT, check=False, stdout=subprocess.DEVNULL,
            env={**os.environ, "DOTTIE_RECORD": str(recorded)},
        )
        raw = json.loads(recorded.read_text())
    fields = gmgn_fields()
    out = {}
    for key, value in raw.items():
        if key.startswith("gmgn"):
            value = slim(value, fields)
        elif key.startswith("rpc:"):
            value = slim(value, RPC_FIELDS)
        elif key.startswith("fomo:"):
            value = drop(value, FOMO_DROP)
        elif key.startswith("jupiter:"):
            value = drop(value, {"mostReliableAmmsQuoteReport", "contextSlot", "timeTaken"})
        out[key] = value
    # Replay freezes the app's clock here, so "the last hour" stays put.
    out["$recorded_at"] = started
    text = json.dumps(out, separators=(",", ":"), ensure_ascii=False, sort_keys=True)
    for name in SECRETS:
        secret = os.environ.get(name, "")
        if len(secret) >= 8 and secret in text:
            raise SystemExit(f"{name} leaked into the fixture; not written")
    OUT.write_text(text)
    print(f"{len(out)} responses, {OUT.stat().st_size // 1024} KB -> {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()

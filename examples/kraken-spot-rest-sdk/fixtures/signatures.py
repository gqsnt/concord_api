"""Independent offline oracle: Python stdlib, no SDK imports or source reads.

The official known answer comes from the Kraken authentication guide, read
2026-11-07. Synthetic inputs are specified here independently of the Rust models.
This script checks fixtures; it does not update them to match SDK output.
"""

import base64
import hashlib
import hmac
import json
from pathlib import Path
from urllib.parse import urlencode


FIXTURES = Path(__file__).resolve().parent
SECRET = b"fixture-signing-secret"


def sign(secret, path, nonce, payload):
    digest = hashlib.sha256(str(nonce).encode("ascii") + payload.encode("ascii")).digest()
    return base64.b64encode(
        hmac.new(secret, path.encode("ascii") + digest, hashlib.sha512).digest()
    ).decode("ascii")


def check(row, path, nonce, fields, secret=SECRET):
    payload = urlencode([("nonce", str(nonce))] + fields)
    assert row["path"] == path
    assert row["payload"] == payload
    assert row["signature"] == sign(secret, path, nonce, payload)


def main():
    wire = json.loads((FIXTURES / "wire-vectors.json").read_text())
    assert wire["key"] == "fixture-key"
    assert base64.b64decode(wire["secret"], validate=True) == SECRET
    balance, orders = wire["vectors"]
    check(balance, "/0/private/Balance", 9001, [
        ("otp", "a+b &=/☃"), ("rebase_multiplier", "base")])
    check(orders, "/0/private/OpenOrders", 9002, [
        ("trades", "true"), ("userref", "-7"), ("cl_ord_id", "client +&=/"),
        ("consolidate_taker", "false"), ("rebase_multiplier", "rebased")])

    account = json.loads((FIXTURES / "account-signatures.json").read_text())
    check(account[0], "/0/private/Balance", 5001, [])
    check(account[1], "/0/private/OpenOrders", 5002, [])
    assert len(account) == 2

    official = json.loads((FIXTURES / "official-signature.json").read_text())
    expected = (
        "4/dpxb3iT4tp/ZCVEwSnEsLxx0bqyhLpdfOpc6fn7OR8+UClSV5n9E6aSS8M"
        "PtnRfp32bAb0nmbRn6H8ndwLUQ=="
    )
    assert official["signature"] == expected
    assert sign(base64.b64decode(official["secret"], validate=True),
                official["path"], official["nonce"], official["payload"]) == expected
    print("Signature fixtures match independent Python oracle: 5 vectors.")


if __name__ == "__main__":
    main()

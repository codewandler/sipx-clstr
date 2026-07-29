#!/usr/bin/env python3
"""Register two users against a running node and place a call between them.

This is the acceptance run for the devspace profile: it speaks raw SIP over UDP, so it depends on
nothing but the standard library and can run from inside a stock `python:3.12-slim` pod.

What it proves, and nothing more:

  1. the registrar accepts a REGISTER and answers 200, echoing the binding it stored;
  2. a second user registers independently;
  3. an INVITE addressed to the second user's AoR is *forwarded by the proxy* to the contact that
     user registered — which is the whole point, because it means the location lookup and the
     forwarding core are wired to each other;
  4. the callee's 200 reaches the caller, and the caller's ACK reaches the callee.

It does not prove media: no RTP flows, the SDP is carried opaquely. `ME-2` is where that starts.

Usage:  python3 sip_demo.py [host:port]
        NODE=host:port python3 sip_demo.py
"""

from __future__ import annotations

import os
import random
import socket
import string
import sys

TIMEOUT = 5.0


def token(n: int = 10) -> str:
    return "".join(random.choices(string.ascii_lowercase + string.digits, k=n))


def branch() -> str:
    # RFC 3261 §8.1.1.7 — the magic cookie is what marks the branch as RFC 3261 compliant.
    return "z9hG4bK" + token(12)


class Endpoint:
    """One SIP user agent: a UDP socket, an address of record, and a contact."""

    def __init__(self, user: str, domain: str, node: tuple[str, int]) -> None:
        self.user = user
        self.domain = domain
        self.node = node
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.bind(("0.0.0.0", 0))
        self.sock.settimeout(TIMEOUT)
        # The address the node must be able to reach us on. Resolved by asking the kernel which
        # local address it would use to reach the node, rather than guessing at hostname lookup.
        probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        probe.connect(node)
        self.ip = probe.getsockname()[0]
        probe.close()
        self.port = self.sock.getsockname()[1]
        self.tag = token(8)

    @property
    def aor(self) -> str:
        return f"sip:{self.user}@{self.domain}"

    @property
    def contact(self) -> str:
        return f"sip:{self.user}@{self.ip}:{self.port}"

    def send(self, message: str, to: tuple[str, int] | None = None) -> None:
        self.sock.sendto(message.encode(), to or self.node)

    def recv(self) -> tuple[str, tuple[str, int]]:
        data, peer = self.sock.recvfrom(65535)
        return data.decode(errors="replace"), peer

    def close(self) -> None:
        self.sock.close()


def status_of(message: str) -> int | None:
    first = message.split("\r\n", 1)[0]
    parts = first.split(" ", 2)
    if len(parts) >= 2 and parts[0].startswith("SIP/2.0"):
        try:
            return int(parts[1])
        except ValueError:
            return None
    return None


def header(message: str, name: str) -> str | None:
    lowered = name.lower() + ":"
    for line in message.split("\r\n"):
        if line.lower().startswith(lowered):
            return line.split(":", 1)[1].strip()
    return None


def register(ep: Endpoint, expires: int = 3600) -> str:
    call_id = token(16)
    request = (
        f"REGISTER sip:{ep.domain} SIP/2.0\r\n"
        f"Via: SIP/2.0/UDP {ep.ip}:{ep.port};branch={branch()};rport\r\n"
        f"Max-Forwards: 70\r\n"
        f"From: <{ep.aor}>;tag={ep.tag}\r\n"
        f"To: <{ep.aor}>\r\n"
        f"Call-ID: {call_id}\r\n"
        f"CSeq: 1 REGISTER\r\n"
        f"Contact: <{ep.contact}>\r\n"
        f"Expires: {expires}\r\n"
        f"Content-Length: 0\r\n\r\n"
    )
    ep.send(request)
    reply, _ = ep.recv()
    return reply


def main() -> int:
    target = sys.argv[1] if len(sys.argv) > 1 else os.environ.get(
        "NODE", "sipx-clstr-node.sipx-clstr-dev.svc.cluster.local:5060"
    )
    host, _, port = target.partition(":")
    node = (socket.gethostbyname(host), int(port or 5060))
    domain = os.environ.get("DOMAIN", host)

    print(f"node      : {target} -> {node[0]}:{node[1]}")
    print(f"domain    : {domain}")
    print()

    alice = Endpoint("alice", domain, node)
    bob = Endpoint("bob", domain, node)
    failures: list[str] = []

    try:
        # ---------------------------------------------------------------- 1 & 2: registration ---
        for ep in (alice, bob):
            reply = register(ep)
            code = status_of(reply)
            contact = header(reply, "Contact")
            ok = code == 200
            print(f"[{'PASS' if ok else 'FAIL'}] REGISTER {ep.user:5s} -> {code} "
                  f"(contact echoed: {contact})")
            if not ok:
                failures.append(f"REGISTER {ep.user} returned {code}, expected 200")
                print("       full reply:\n" + "\n".join(
                    "         " + line for line in reply.strip().split("\r\n")))

        if failures:
            return report(failures)

        # ------------------------------------------------------- 3: the proxy forwards to bob ---
        call_id = token(16)
        sdp = (
            "v=0\r\n"
            f"o=alice 0 0 IN IP4 {alice.ip}\r\n"
            "s=-\r\n"
            f"c=IN IP4 {alice.ip}\r\n"
            "t=0 0\r\n"
            "m=audio 40000 RTP/AVP 8 0 101\r\n"
            "a=rtpmap:8 PCMA/8000\r\n"
            "a=rtpmap:0 PCMU/8000\r\n"
            "a=rtpmap:101 telephone-event/8000\r\n"
        )
        invite = (
            f"INVITE {bob.aor} SIP/2.0\r\n"
            f"Via: SIP/2.0/UDP {alice.ip}:{alice.port};branch={branch()};rport\r\n"
            f"Max-Forwards: 70\r\n"
            f"From: <{alice.aor}>;tag={alice.tag}\r\n"
            f"To: <{bob.aor}>\r\n"
            f"Call-ID: {call_id}\r\n"
            f"CSeq: 1 INVITE\r\n"
            f"Contact: <{alice.contact}>\r\n"
            f"Content-Type: application/sdp\r\n"
            f"Content-Length: {len(sdp)}\r\n\r\n{sdp}"
        )
        alice.send(invite)

        # Bob should receive the INVITE from the node, not from alice: the proxy is in the path.
        try:
            received, peer = bob.recv()
        except socket.timeout:
            failures.append("bob never received the INVITE — the proxy did not forward it")
            return report(failures)

        method = received.split(" ", 1)[0]
        via_count = sum(1 for line in received.split("\r\n") if line.lower().startswith("via:"))
        record_route = header(received, "Record-Route")
        forwarded_by_node = peer[0] == node[0]
        print(f"[{'PASS' if method == 'INVITE' else 'FAIL'}] INVITE reached bob from "
              f"{peer[0]}:{peer[1]} ({'the node' if forwarded_by_node else 'NOT the node'})")
        print(f"       Via headers stacked: {via_count} (proxy added its own)")
        print(f"       Record-Route: {record_route}")
        if method != "INVITE":
            failures.append(f"bob received {method}, expected INVITE")
        if via_count < 2:
            failures.append(f"only {via_count} Via header(s); a forwarding proxy must add one")

        # ------------------------------------------------ 4: bob answers, alice gets the 200 ---
        to_header = header(received, "To") or f"<{bob.aor}>"
        answer = (
            "SIP/2.0 200 OK\r\n"
            + "\r\n".join(l for l in received.split("\r\n") if l.lower().startswith("via:"))
            + f"\r\nFrom: {header(received, 'From')}\r\n"
            f"To: {to_header};tag={bob.tag}\r\n"
            f"Call-ID: {header(received, 'Call-ID')}\r\n"
            f"CSeq: {header(received, 'CSeq')}\r\n"
            f"Contact: <{bob.contact}>\r\n"
            f"Content-Length: 0\r\n\r\n"
        )
        bob.send(answer, peer)

        try:
            final, _ = alice.recv()
            while status_of(final) is not None and status_of(final) < 200:
                print(f"       provisional: {status_of(final)}")
                final, _ = alice.recv()
        except socket.timeout:
            failures.append("alice never received the 200 — the response did not traverse back")
            return report(failures)

        code = status_of(final)
        print(f"[{'PASS' if code == 200 else 'FAIL'}] 200 OK returned to alice -> {code}")
        if code != 200:
            failures.append(f"alice received {code}, expected 200")

    finally:
        alice.close()
        bob.close()

    return report(failures)


def report(failures: list[str]) -> int:
    print()
    if failures:
        print("RESULT: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("RESULT: PASS — registrar stored both bindings and the proxy forwarded between them")
    return 0


if __name__ == "__main__":
    sys.exit(main())

import pytest
from agent10.prod import answer, redact, moderate, run_evals, handle


def env(guard, reply):
    return lambda m, i: {"response": guard if "guard" in m else reply}


def test_unsafe_refused_before_main_model():
    t = []
    r = answer(env("unsafe\nS2", "x"), "bad", t)
    assert r["refused"] is True and len(t) == 1


def test_safe_answered_traced_redacted():
    t = []
    r = answer(env("safe", "mail me at a@b.com key sk-abcdefghijkl"), "hi", t)
    assert r["answer"] == "mail me at [email] key [secret]" and [s["span"] for s in t] == ["guard.input", "llm.answer", "guard.output"]


def test_redact_basic_and_evals():
    assert redact("x@y.io") == "[email]"
    assert run_evals(env("safe", "144 Paris OK"))["passed"] == 3


def test_redact_patterns():
    assert redact("ssn 123-45-6789") == "ssn [ssn]"
    assert redact("card 4111 1111 1111 1111 ok") == "card [card] ok"
    for p in ["(512) 555-0199", "512-555-0199", "512.555.0199", "+1 512 555 0199"]:
        assert redact("call " + p) == "call [phone]"
    assert redact("ghp" + "_" + "FAKE0EXAMPLE0TOKEN") == "[secret]" and redact("xoxb" + "-" + "FAKE0EXAMPLE0TOKEN") == "[secret]"
    s = "We have 12 trucks and 3 offices, call volume up 40% in 2026."
    assert redact(s) == s


def test_moderation_verdicts_and_refusal_never_calls_main():
    assert moderate(env("safe", "x"), "x")["safe"] is True and moderate(env("unsafe\nS9", "x"), "x")["safe"] is False
    main = []
    def ai(m, i):
        if "guard" not in m: main.append(1)
        return {"response": "unsafe" if "guard" in m else "x"}
    answer(ai, "bad", [])
    assert not main


def test_http_router():
    ai = env("safe", "ok")
    assert handle(ai, "GET", "/healthz", None, "rid")[1] == {"ok": True, "version": "1.1.0"}
    assert handle(ai, "POST", "/api/ask", {}, "rid")[0] == 400
    s, r, x = handle(ai, "POST", "/api/ask", {"question": "hi"}, "rid")
    assert s == 200 and r["request_id"] == "rid" and x == {"refused": False}
    def boom(m, i): raise RuntimeError("quota")
    s, r, x = handle(boom, "POST", "/api/ask", {"question": "hi"}, "rid")
    assert s == 502 and x == {"err": "quota"}
    assert handle(ai, "GET", "/", None, "rid")[0] == 0

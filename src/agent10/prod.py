"""Production-style agent: input guardrail (Llama Guard), traced spans, output redaction, request ids and a tiny eval suite."""
import json, re, time

MODEL = "@cf/meta/llama-3.3-70b-instruct-fp8-fast"
GUARD = "@cf/meta/llama-guard-3-8b"
VERSION = "1.1.0"
SYSTEM = "You are a concise, accurate assistant for a small-business AI agency. Answer in under 120 words."
A = re.ASCII
_REDACT = [
    (re.compile(r"[\w.+-]+@[\w-]+\.[\w.]+", A), "[email]"),
    (re.compile(r"\b(sk|pk|ghp|xox[bp])[-_][A-Za-z0-9_-]{8,}\b", A), "[secret]"),
    (re.compile(r"\b\d{3}-\d{2}-\d{4}\b", A), "[ssn]"),
    (re.compile(r"\b\d(?:[ -]?\d){12,15}\b", A), "[card]"),
    (re.compile(r"(?:\+?1[ .-]?)?\(?\b\d{3}\)?[ .-]?\d{3}[ .-]?\d{4}\b", A), "[phone]"),
]
dumps = lambda o: json.dumps(o, separators=(",", ":"), ensure_ascii=False)


def text(r):
    v = r.get("response") if isinstance(r, dict) else None
    return v if isinstance(v, str) else dumps(v if v is not None else r)


def cap(s, n=500):
    return isinstance(s, str) and bool(s.strip()) and len(s) <= n


def redact(s):
    s = str(s)
    for rx, rep in _REDACT:
        s = rx.sub(rep, s)
    return s


def moderate(ai, text_in):
    r = text(ai(GUARD, {"messages": [{"role": "user", "content": text_in}]}))
    return {"safe": re.search("unsafe", r, re.I) is None, "raw": r[:60]}


def answer(ai, question, trace, now=lambda: time.time() * 1000):
    def span(name, fn):
        t0 = now()
        try:
            return fn()
        finally:
            trace.append({"span": name, "ms": round(now() - t0)})
    mod = span("guard.input", lambda: moderate(ai, question))
    if not mod["safe"]:
        return {"refused": True, "answer": "Sorry, I can't help with that request."}
    out = span("llm.answer", lambda: text(ai(MODEL, {"messages": [{"role": "system", "content": SYSTEM}, {"role": "user", "content": question}], "max_tokens": 250})))
    return {"refused": False, "answer": span("guard.output", lambda: redact(out))}


EVALS = [
    ("What is 12 times 12? Reply with just the number.", lambda a: re.search(r"\b144\b", a, A) is not None),
    ("What is the capital of France? One word.", lambda a: re.search("paris", a, re.I) is not None),
    ("Reply with exactly the word: OK", lambda a: re.search(r"\bok\b", a, A | re.I) is not None),
]


def run_evals(ai):
    results = []
    for q, check in EVALS:
        r = answer(ai, q, [])
        results.append({"q": q, "pass": (not r["refused"]) and check(r["answer"]), "answer": r["answer"][:80]})
    return {"passed": sum(1 for r in results if r["pass"]), "total": len(results), "results": results}


def handle(ai, method, path, body, rid):
    """Pure router: (status, json, log_extra); status 0 = serve the page."""
    if path == "/healthz":
        return 200, {"ok": True, "version": VERSION}, {}
    if path == "/api/evals":
        return 200, run_evals(ai), {}
    if method == "POST" and path == "/api/ask":
        q = body.get("question") if isinstance(body, dict) else None
        if not cap(q):
            return 400, {"error": "question required (<=500 chars)"}, {}
        trace = []
        try:
            r = answer(ai, q, trace)
            return 200, {"request_id": rid, **r, "trace": trace}, {"refused": r["refused"]}
        except Exception as e:
            return 502, {"request_id": rid, "error": "Upstream model error, please retry."}, {"err": str(e)}
    return 0, None, {}

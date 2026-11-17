//! Production-style agent: input guardrail (Llama Guard), traced spans, output redaction, request ids and a tiny eval suite.
use regex::Regex;
use serde_json::{json, Value};

pub const MODEL: &str = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";
pub const GUARD: &str = "@cf/meta/llama-guard-3-8b";
pub const VERSION: &str = "1.1.0";
pub const SYSTEM: &str = "You are a concise, accurate assistant for a small-business AI agency. Answer in under 120 words.";

#[allow(async_fn_in_trait)]
pub trait Ai {
    async fn run(&self, model: &str, input: Value) -> Result<Value, String>;
}

pub fn text(r: &Value) -> String {
    match r.get("response") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => r.to_string(),
        Some(v) => v.to_string(),
    }
}
pub fn cap(s: Option<&str>, n: usize) -> bool {
    matches!(s, Some(s) if !s.trim().is_empty() && s.chars().count() <= n)
}

/// Output guardrail: strip anything that looks like a secret or an email. ASCII word rules, as in JS.
pub fn redact(s: &str) -> String {
    let rules: [(&str, &str); 5] = [
        (r"(?-u)[\w.+-]+@[\w-]+\.[\w.]+", "[email]"),
        (r"(?-u)\b(sk|pk|ghp|xox[bp])[-_][A-Za-z0-9_-]{8,}\b", "[secret]"),
        (r"(?-u)\b\d{3}-\d{2}-\d{4}\b", "[ssn]"),
        (r"(?-u)\b\d(?:[ -]?\d){12,15}\b", "[card]"),
        (r"(?-u)(?:\+?1[ .-]?)?\(?\b\d{3}\)?[ .-]?\d{3}[ .-]?\d{4}\b", "[phone]"),
    ];
    let mut out = s.to_string();
    for (re, rep) in rules {
        out = Regex::new(re).unwrap().replace_all(&out, rep).into_owned();
    }
    out
}

/// Input guardrail: Llama Guard classifies the prompt; unsafe input is refused before the main model runs.
pub async fn moderate<A: Ai>(ai: &A, input: &str) -> Result<(bool, String), String> {
    let r = text(&ai.run(GUARD, json!({"messages":[{"role":"user","content":input}]})).await?);
    let safe = !r.to_lowercase().contains("unsafe");
    Ok((safe, r.chars().take(60).collect()))
}

/// `now` returns ms. Spans are pushed even when a step fails, like the JS `finally`.
pub async fn answer<A: Ai>(ai: &A, question: &str, trace: &mut Vec<Value>, now: &dyn Fn() -> i64) -> Result<Value, String> {
    let t0 = now();
    let m = moderate(ai, question).await;
    trace.push(json!({"span": "guard.input", "ms": now() - t0}));
    if !m?.0 {
        return Ok(json!({"refused": true, "answer": "Sorry, I can't help with that request."}));
    }
    let t1 = now();
    let o = ai.run(MODEL, json!({"messages":[{"role":"system","content":SYSTEM},{"role":"user","content":question}],"max_tokens":250})).await;
    trace.push(json!({"span": "llm.answer", "ms": now() - t1}));
    let out = text(&o?);
    let t2 = now();
    let red = redact(&out);
    trace.push(json!({"span": "guard.output", "ms": now() - t2}));
    Ok(json!({"refused": false, "answer": red}))
}

pub async fn run_evals<A: Ai>(ai: &A, now: &dyn Fn() -> i64) -> Result<Value, String> {
    let evals: [(&str, &str); 3] = [
        ("What is 12 times 12? Reply with just the number.", r"(?-u)\b144\b"),
        ("What is the capital of France? One word.", r"(?i)paris"),
        ("Reply with exactly the word: OK", r"(?i)(?-u)\bok\b"),
    ];
    let mut results = vec![];
    for (q, re) in evals {
        let r = answer(ai, q, &mut vec![], now).await?;
        let a = r["answer"].as_str().unwrap_or("");
        let pass = r["refused"] == false && Regex::new(re).unwrap().is_match(a);
        results.push(json!({"q": q, "pass": pass, "answer": a.chars().take(80).collect::<String>()}));
    }
    let passed = results.iter().filter(|r| r["pass"] == true).count();
    Ok(json!({"passed": passed, "total": results.len(), "results": results}))
}

/// Pure router: (status, json, log_extra); status 0 = serve the page.
pub async fn handle<A: Ai>(ai: &A, method: &str, path: &str, body: &Value, rid: &str, now: &dyn Fn() -> i64) -> (u16, Value, Value) {
    if path == "/healthz" {
        return (200, json!({"ok": true, "version": VERSION}), json!({}));
    }
    if path == "/api/evals" {
        return match run_evals(ai, now).await {
            Ok(v) => (200, v, json!({})),
            Err(e) => (500, json!({"error": e}), json!({})),
        };
    }
    if method == "POST" && path == "/api/ask" {
        let q = body.get("question").and_then(|v| v.as_str());
        if !cap(q, 500) {
            return (400, json!({"error": "question required (<=500 chars)"}), json!({}));
        }
        let mut trace = vec![];
        return match answer(ai, q.unwrap(), &mut trace, now).await {
            Ok(r) => {
                let mut o = json!({"request_id": rid});
                for (k, v) in r.as_object().unwrap() {
                    o[k] = v.clone();
                }
                o["trace"] = Value::Array(trace);
                (200, o, json!({"refused": r["refused"]}))
            }
            Err(e) => (502, json!({"request_id": rid, "error": "Upstream model error, please retry."}), json!({"err": e})),
        };
    }
    (0, Value::Null, json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use std::cell::Cell;

    struct Env(&'static str, &'static str, Cell<u32>);
    impl Ai for Env {
        async fn run(&self, m: &str, _i: Value) -> Result<Value, String> {
            if m.contains("guard") { Ok(json!({"response": self.0})) } else { self.2.set(self.2.get() + 1); Ok(json!({"response": self.1})) }
        }
    }
    struct Boom;
    impl Ai for Boom {
        async fn run(&self, _m: &str, _i: Value) -> Result<Value, String> { Err("quota".into()) }
    }
    fn env(g: &'static str, r: &'static str) -> Env { Env(g, r, Cell::new(0)) }
    fn clk() -> impl Fn() -> i64 { let c = Cell::new(0i64); move || { c.set(c.get() + 2); c.get() } }

    #[test]
    fn guardrails_refuse_trace_redact() {
        let mut t = vec![];
        let e = env("unsafe\nS2", "x");
        let r = block_on(answer(&e, "bad", &mut t, &clk())).unwrap();
        assert_eq!(r["refused"], true);
        assert_eq!((t.len(), e.2.get()), (1, 0));
        let mut t = vec![];
        let r = block_on(answer(&env("safe", "mail me at a@b.com key sk-abcdefghijkl"), "hi", &mut t, &clk())).unwrap();
        assert_eq!(r["answer"], "mail me at [email] key [secret]");
        assert_eq!(t.iter().map(|s| s["span"].as_str().unwrap()).collect::<Vec<_>>(), ["guard.input", "llm.answer", "guard.output"]);
        assert_eq!(block_on(run_evals(&env("safe", "144 Paris OK"), &clk())).unwrap()["passed"], 3);
    }
    #[test]
    fn redact_patterns() {
        assert_eq!(redact("x@y.io"), "[email]");
        assert_eq!(redact("ssn 123-45-6789"), "ssn [ssn]");
        assert_eq!(redact("card 4111 1111 1111 1111 ok"), "card [card] ok");
        for p in ["(512) 555-0199", "512-555-0199", "512.555.0199", "+1 512 555 0199"] {
            assert_eq!(redact(&format!("call {p}")), "call [phone]");
        }
        assert_eq!(redact(&format!("ghp{}FAKE0EXAMPLE0TOKEN", "_")), "[secret]");
        assert_eq!(redact(&format!("xoxb{}FAKE0EXAMPLE0TOKEN", "-")), "[secret]");
        let s = "We have 12 trucks and 3 offices, call volume up 40% in 2026.";
        assert_eq!(redact(s), s);
    }
    #[test]
    fn moderation_and_http() {
        assert!(block_on(moderate(&env("safe", "x"), "x")).unwrap().0);
        assert!(!block_on(moderate(&env("unsafe\nS9", "x"), "x")).unwrap().0);
        let e = env("safe", "ok");
        assert_eq!(block_on(handle(&e, "GET", "/healthz", &json!({}), "rid", &clk())).1, json!({"ok": true, "version": "1.1.0"}));
        assert_eq!(block_on(handle(&e, "POST", "/api/ask", &json!({}), "rid", &clk())).0, 400);
        let (s, r, x) = block_on(handle(&e, "POST", "/api/ask", &json!({"question": "hi"}), "rid", &clk()));
        assert_eq!((s, r["request_id"].as_str().unwrap(), x), (200, "rid", json!({"refused": false})));
        let (s, _, x) = block_on(handle(&Boom, "POST", "/api/ask", &json!({"question": "hi"}), "rid", &clk()));
        assert_eq!((s, x), (502, json!({"err": "quota"})));
        assert_eq!(block_on(handle(&e, "GET", "/", &json!({}), "rid", &clk())).0, 0);
    }
}

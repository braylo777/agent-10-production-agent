// Parity helper: same redaction corpus and scenarios as the legacy JS harness; prints JSON.
use agent_10_production_agent::prod::*;
use futures::executor::block_on;
use serde_json::{json, Value};
struct E(&'static str, &'static str);
impl Ai for E {
    async fn run(&self, m: &str, _i: Value) -> Result<Value, String> { Ok(json!({"response": if m.contains("guard") { self.0 } else { self.1 }})) }
}
fn main() {
    let c: Vec<String> = serde_json::from_str(&std::fs::read_to_string("/tmp/red10.json").unwrap()).unwrap();
    let now = || 0i64;
    let red: Vec<String> = c.iter().map(|s| redact(s)).collect();
    let mut m = vec![];
    for g in ["safe", "unsafe\nS9", "UNSAFE", "Unsafe content here and then some text beyond sixty characters long enough to truncate", "weird"] {
        let (safe, raw) = block_on(moderate(&E(g, "x"), "q")).unwrap();
        m.push(json!({"safe": safe, "raw": raw}));
    }
    let mut ans = vec![];
    for (g, r) in [("safe", "mail a@b.com sk-abcdefghijkl 512-555-0199"), ("unsafe", "x"), ("safe", "")] {
        let mut t = vec![];
        let r = block_on(answer(&E(g, r), "q", &mut t, &now)).unwrap();
        ans.push(json!({"r": r, "spans": t.iter().map(|s| s["span"].clone()).collect::<Vec<_>>()}));
    }
    let evals: Vec<Value> = [("safe", "144 Paris OK"), ("safe", "no"), ("unsafe", "x"), ("safe", "paris 1440 okay")].iter().map(|(g, r)| block_on(run_evals(&E(g, r), &now)).unwrap()).collect();
    println!("{}", serde_json::to_string(&json!({"red": red, "mod": m, "ans": ans, "evals": evals})).unwrap());
}

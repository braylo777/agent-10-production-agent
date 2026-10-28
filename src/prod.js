import { ask, text, chat } from "./core.js";

export const GUARD = "@cf/meta/llama-guard-3-8b";
// Input guardrail: Llama Guard classifies the prompt; unsafe input is refused before the main model runs.
export async function moderate(env, input) {
  const r = text(await env.AI.run(GUARD, { messages: [{ role: "user", content: input }] }));
  return { safe: !/unsafe/i.test(r), raw: r.slice(0, 60) };
}
// Output guardrail: strip anything that looks like a secret or an email before returning.
export const redact = (s) => String(s).replace(/[\w.+-]+@[\w-]+\.[\w.]+/g, "[email]").replace(/\b(sk|pk|ghp|xox[bp])[-_][A-Za-z0-9_-]{8,}\b/g, "[secret]");

export async function answer(env, question, trace) {
  const span = async (name, fn) => { const t0 = Date.now(); try { return await fn(); } finally { trace.push({ span: name, ms: Date.now() - t0 }); } };
  const mod = await span("guard.input", () => moderate(env, question));
  if (!mod.safe) return { refused: true, answer: "Sorry, I can't help with that request." };
  const out = await span("llm.answer", () => ask(env, "You are a concise, accurate assistant for a small-business AI agency. Answer in under 120 words.", question, { max_tokens: 250 }));
  return { refused: false, answer: await span("guard.output", async () => redact(out)) };
}

// Tiny offline-gradable eval suite: run with GET /api/evals (heavily rate limited).
export const EVALS = [
  { q: "What is 12 times 12? Reply with just the number.", check: (a) => /\b144\b/.test(a) },
  { q: "What is the capital of France? One word.", check: (a) => /paris/i.test(a) },
  { q: "Reply with exactly the word: OK", check: (a) => /\bok\b/i.test(a) },
];
export async function runEvals(env) {
  const results = [];
  for (const e of EVALS) { const r = await answer(env, e.q, []); results.push({ q: e.q, pass: !r.refused && e.check(r.answer), answer: r.answer.slice(0, 80) }); }
  return { passed: results.filter((r) => r.pass).length, total: results.length, results };
}

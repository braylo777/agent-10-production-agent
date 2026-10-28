import { test } from "node:test";
import assert from "node:assert/strict";
import { answer, redact, runEvals } from "../src/prod.js";

const env = (guard, reply) => ({ AI: { run: async (m) => ({ response: m.includes("guard") ? guard : reply }) } });
test("unsafe input is refused before main model", async () => {
  const t = []; const r = await answer(env("unsafe\nS2", "x"), "bad", t); assert.equal(r.refused, true); assert.equal(t.length, 1);
});
test("safe input answered, traced, redacted", async () => {
  const t = []; const r = await answer(env("safe", "mail me at a@b.com key sk-abcdefghijkl"), "hi", t);
  assert.equal(r.answer, "mail me at [email] key [secret]"); assert.deepEqual(t.map((s) => s.span), ["guard.input", "llm.answer", "guard.output"]);
});
test("redact", () => assert.equal(redact("x@y.io"), "[email]"));
test("evals grade answers", async () => { const r = await runEvals(env("safe", "144 Paris OK")); assert.equal(r.passed, 3); });

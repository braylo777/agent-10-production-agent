import { test } from "node:test";
import assert from "node:assert/strict";
import { answer, redact, moderate } from "../src/prod.js";

const env = (guard, reply) => ({ AI: { run: async (m) => ({ response: m.includes("guard") ? guard : reply }) } });
test("redacts SSN", () => assert.equal(redact("ssn 123-45-6789"), "ssn [ssn]"));
test("redacts card numbers", () => assert.equal(redact("card 4111 1111 1111 1111 ok"), "card [card] ok"));
test("redacts US phone formats", () => { for (const p of ["(512) 555-0199", "512-555-0199", "512.555.0199", "+1 512 555 0199"]) assert.equal(redact("call " + p), "call [phone]"); });
test("redacts GitHub and Slack tokens", () => { assert.equal(redact("ghp_abcdefghij123"), "[secret]"); assert.equal(redact("xoxb-123456789abc"), "[secret]"); });
test("leaves ordinary text and small numbers alone", () => { const s = "We have 12 trucks and 3 offices, call volume up 40% in 2026."; assert.equal(redact(s), s); });
test("moderation parses Llama Guard verdicts", async () => {
  assert.equal((await moderate(env("safe"), "x")).safe, true); assert.equal((await moderate(env("unsafe\nS9"), "x")).safe, false);
});
test("refusal never calls the main model", async () => {
  let main = 0; const e = { AI: { run: async (m) => { if (!m.includes("guard")) main++; return { response: m.includes("guard") ? "unsafe" : "x" }; } } };
  await answer(e, "bad", []); assert.equal(main, 0);
});

import { json, limited, body, cap } from "./core.js";
import { answer, runEvals } from "./prod.js";
import page from "./page.html";

export default {
  async fetch(req, env) {
    const id = crypto.randomUUID().slice(0, 12), t0 = Date.now(), url = new URL(req.url);
    const done = (res, extra = {}) => {
      // Structured log line per request -> Workers Logs (observability enabled).
      console.log(JSON.stringify({ id, path: url.pathname, status: res.status, ms: Date.now() - t0, ...extra }));
      res.headers.set("x-request-id", id); res.headers.set("server-timing", `total;dur=${Date.now() - t0}`); return res;
    };
    if (url.pathname === "/healthz") return done(json({ ok: true, version: "1.0.0" }));
    if (url.pathname === "/api/evals") {
      if (await limited(req, env)) return done(json({ error: "Rate limit: try again in a minute." }, 429));
      return done(json(await runEvals(env)));
    }
    if (req.method === "POST" && url.pathname === "/api/ask") {
      if (await limited(req, env)) return done(json({ error: "Rate limit: try again in a minute." }, 429));
      const { question } = await body(req);
      if (!cap(question)) return done(json({ error: "question required (<=500 chars)" }, 400));
      const trace = [];
      try { const r = await answer(env, question, trace); return done(json({ request_id: id, ...r, trace }), { refused: r.refused }); }
      catch (e) { return done(json({ request_id: id, error: "Upstream model error, please retry." }, 502), { err: e.message }); }
    }
    return done(new Response(page, { headers: { "content-type": "text/html;charset=utf-8" } }));
  },
};

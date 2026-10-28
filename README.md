# agent-10-production-agent

A **production-grade AI agent**, the capstone of a 10-part agentic AI portfolio. It shows what changes between a demo and something you can ship: guardrails, observability, evals, rate limits and CI. It runs serverless on Cloudflare Workers with the free Workers AI tier.

**Live demo:** https://agent-10-production-agent.braylo777.workers.dev

## What it is
A Q&A agent wrapped in production plumbing: Llama Guard input moderation, output redaction (emails and API keys), a request id plus span trace on every response, structured JSON logs (Workers Logs), `/healthz`, an eval suite at `/api/evals`, per-IP rate limiting, and GitHub Actions CI.

## How it works
```
request ─▶ rate limit ─▶ validate ─▶ [span] Llama Guard ─unsafe─▶ refuse
                                        │ safe
                                   [span] LLM answer ─▶ [span] redact ─▶ response
 every response: x-request-id, server-timing, JSON log line
```
- `src/prod.js`: guardrails, traced spans, and the eval suite with programmatic graders.
- `src/index.js`: request ids, structured logging, health check, error handling that never leaks internals.
- `ci/github-actions-ci.yml`: GitHub Actions workflow (unit tests plus a bundle check). Copy it to `.github/workflows/` to enable it.
- Cost control: 5 requests/min per IP keeps the demo inside the Workers AI free allocation.

## Run it
```bash
npm install
npm test            # offline unit tests (Node 20+), LLM is mocked
npx wrangler dev    # local; Workers AI runs via your Cloudflare login
npx wrangler deploy # free tier, no paid API keys
```

## Stack
Cloudflare Workers · Workers AI (`@cf/meta/llama-3.3-70b-instruct-fp8-fast`, `@cf/meta/llama-guard-3-8b` · GitHub Actions) · vanilla JS · node:test

## License
MIT

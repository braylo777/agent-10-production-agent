//! Cloudflare Worker entry (wasm32 only). Same routes, headers and request logs as the old src/index.js.
use crate::page::PAGE;
use crate::prod::{handle, Ai};
use serde_json::{json, Value};
use wasm_bindgen::JsCast;
use worker::*;

struct CfAi(worker::Ai);
impl Ai for CfAi {
    async fn run(&self, model: &str, input: Value) -> std::result::Result<Value, String> {
        self.0.run::<Value, Value>(model, input).await.map_err(|e| e.to_string())
    }
}

async fn rate_ok(env: &Env, ip: &str) -> bool {
    let rl = match js_sys::Reflect::get(env.as_ref(), &"RATE_LIMITER".into()) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return true,
    };
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &"key".into(), &ip.into());
    let Ok(f) = js_sys::Reflect::get(&rl, &"limit".into()).and_then(|f| f.dyn_into::<js_sys::Function>().map_err(|e| e)) else { return true };
    let Ok(p) = f.call1(&rl, &opts) else { return true };
    match wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(p)).await {
        Ok(r) => js_sys::Reflect::get(&r, &"success".into()).ok().and_then(|v| v.as_bool()).unwrap_or(true),
        Err(_) => true,
    }
}

/// 12 chars shaped like uuid.slice(0, 12): 8 hex, a dash, 3 hex.
fn request_id() -> String {
    let h = |n: usize| -> String { (0..n).map(|_| char::from_digit((js_sys::Math::random() * 16.0) as u32 % 16, 16).unwrap()).collect() };
    format!("{}-{}", h(8), h(3))
}

#[event(fetch)]
async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let (id, t0) = (request_id(), js_sys::Date::now());
    let url = req.url()?;
    let path = url.path().to_string();
    let method = req.method();
    let done = |res: Response, status: u16, extra: Value| -> Result<Response> {
        let ms = (js_sys::Date::now() - t0) as i64;
        let mut line = json!({"id": id, "path": path, "status": status, "ms": ms});
        if let Some(o) = extra.as_object() {
            for (k, v) in o {
                line[k] = v.clone();
            }
        }
        console_log!("{}", line);
        let mut res = res;
        res.headers_mut().set("x-request-id", &id)?;
        res.headers_mut().set("server-timing", &format!("total;dur={ms}"))?;
        Ok(res)
    };
    if path == "/api/evals" || (method == Method::Post && path == "/api/ask") {
        let ip = req.headers().get("cf-connecting-ip")?.unwrap_or_else(|| "anon".into());
        if !rate_ok(&env, &ip).await {
            return done(Response::from_json(&json!({"error":"Rate limit: try again in a minute."}))?.with_status(429), 429, json!({}));
        }
    }
    let body: Value = if method == Method::Post { req.json().await.unwrap_or(json!({})) } else { json!({}) };
    let ai = CfAi(env.ai("AI")?);
    let now = || js_sys::Date::now() as i64;
    let (status, v, extra) = handle(&ai, method.as_ref(), &path, &body, &id, &now).await;
    if status == 0 {
        let h = Headers::new();
        h.set("content-type", "text/html;charset=utf-8")?;
        return done(Response::ok(PAGE)?.with_headers(h), 200, json!({}));
    }
    done(Response::from_json(&v)?.with_status(status), status, extra)
}

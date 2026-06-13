//! `fetch(url)` -- net-cat call wrapped as a boa-cat `NativeFn`
//! returning a `Promise<Response>`.  v0.7.1 upgraded to the
//! Promise-based DOM Fetch API shape so scripts can use
//! `fetch(url).then(r => r.text()).then(t => ...)` and
//! `await fetch(url)` via the engine's microtask driver.
//!
//! Response shape:
//!
//! - `ok` (bool)
//! - `status` (number)
//! - `statusText` (string)
//! - `headers` (object: lowercased name -> value)
//! - `text()` -> `Promise<string>` (v0.7.1)
//! - `json()` -> `Promise<any>` -- currently a rejected stub
//!   (v0.7.1).  Scripts should use `JSON.parse(await r.text())`
//!   instead while a Rust-side JSON parser is out of scope for
//!   this crate.  `ecma-runtime-cat` already exposes `JSON.parse`
//!   to the engine, so the user path works end-to-end.
//!
//! v0 limitations:
//!
//! - `GET` requests only; body / method deferred.
//! - The underlying transport is still synchronous; the Promise
//!   wrapping is purely a shape change at the boundary so the
//!   engine's existing `.then` / `await` dispatch works.
//!
//! v0.7.3: HTTPS is now supported.  `net-cat` was bumped from
//! `0.1` to `0.3` with the `tls` feature enabled (rustls +
//! webpki-roots + ring), so `https://` URLs flow through the same
//! `perform_fetch` path as `http://` ones.  Cross-origin
//! `Cookie` / `Authorization` stripping on redirect (net-cat
//! 0.3) and chunked transfer decoding both come along for the
//! ride.  Tests in `tests/fetch_promise.rs` stay focused on the
//! rejected-Promise shape; live HTTPS hits a real endpoint and
//! is intentionally out of the test surface to keep the suite
//! offline-deterministic.

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::promise::PromiseState;
use boa_cat::value::{Object, ObjectId};
use net_cat::{Method, Request, Response, Url};

/// The `fetch(url)` native callable.
///
/// # Errors
///
/// Never returns `Err`; success and failure both surface as a
/// settled Promise (`Resolved(Response)` or `Rejected(String)`).
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn fetch_impl(args: Vec<Value>, _this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let url_arg = args.first().and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::Object(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => None,
    });
    let (state, heap) = if let Some(url_str) = url_arg {
        fetch_to_promise_state(&url_str, heap)
    } else {
        (
            PromiseState::Rejected(Value::String(
                "TypeError: fetch() requires a URL string".to_owned(),
            )),
            heap,
        )
    };
    let (promise_id, heap) = heap.alloc_promise(state);
    Ok((Outcome::Normal(Value::Promise(promise_id)), heap, fuel))
}

fn fetch_to_promise_state(url_str: &str, heap: Heap) -> (PromiseState, Heap) {
    match perform_fetch(url_str) {
        Ok(response) => {
            let (value, heap) = build_response_object(&response, heap);
            (PromiseState::Resolved(value), heap)
        }
        Err(err) => (
            PromiseState::Rejected(Value::String(format!("TypeError: fetch failed: {err}"))),
            heap,
        ),
    }
}

fn perform_fetch(url_str: &str) -> Result<Response, net_cat::Error> {
    let url = Url::parse(url_str)?;
    let request = Request::new(Method::Get, url);
    net_cat::fetch(&request)
}

fn build_response_object(response: &Response, heap: Heap) -> (Value, Heap) {
    let status = response.status();
    let ok = (200..300).contains(&status);
    let text = response.body_text();
    let (headers_value, heap) = build_headers_object(response.headers(), heap);
    let mut props = BTreeMap::new();
    let _ = props.insert("ok".to_owned(), Value::Boolean(ok));
    let _ = props.insert("status".to_owned(), Value::Number(f64::from(status)));
    let _ = props.insert(
        "statusText".to_owned(),
        Value::String(response.reason().to_owned()),
    );
    let _ = props.insert("__body__".to_owned(), Value::String(text));
    let _ = props.insert("headers".to_owned(), headers_value);
    let _ = props.insert("text".to_owned(), Value::Native(response_text_impl));
    let _ = props.insert("json".to_owned(), Value::Native(response_json_impl));
    let (id, heap) = heap.alloc_object(Object::from_properties(props));
    (Value::Object(id), heap)
}

fn build_headers_object(headers: &net_cat::Headers, heap: Heap) -> (Value, Heap) {
    let map: BTreeMap<String, Value> = headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), Value::String(v.clone())))
        .collect();
    let (id, heap) = heap.alloc_object(Object::from_properties(map));
    (Value::Object(id), heap)
}

/// `response.text()` (v0.7.1): returns a `Promise<string>` that
/// resolves to the response body.  Body bytes are decoded to UTF-8
/// at fetch time and stashed under a hidden `__body__` slot; this
/// method just wraps that string in a resolved Promise.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn response_text_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let body = read_body(&this, &heap);
    let (promise_id, heap) = heap.alloc_promise(PromiseState::Resolved(Value::String(body)));
    Ok((Outcome::Normal(Value::Promise(promise_id)), heap, fuel))
}

/// `response.json()` (v0.7.1): currently a rejected stub.  A
/// Rust-side JSON parser is out of scope for this crate; scripts
/// should use `JSON.parse(await r.text())` instead, which works
/// because `ecma-runtime-cat` exposes `JSON.parse` to the engine.
/// The rejected promise carries an explanatory message so the
/// failure path is greppable.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn response_json_impl(_args: Vec<Value>, _this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let (promise_id, heap) = heap.alloc_promise(PromiseState::Rejected(Value::String(
        "TypeError: response.json() not yet implemented; use JSON.parse(await r.text()) instead"
            .to_owned(),
    )));
    Ok((Outcome::Normal(Value::Promise(promise_id)), heap, fuel))
}

fn read_body(this: &Value, heap: &Heap) -> String {
    object_id_of(this)
        .and_then(|id| heap.object(id))
        .and_then(|obj| obj.get("__body__").cloned())
        .and_then(|v| match v {
            Value::String(s) => Some(s),
            Value::Undefined
            | Value::Null
            | Value::Boolean(_)
            | Value::Number(_)
            | Value::Object(_)
            | Value::Function(_)
            | Value::Native(_)
            | Value::Promise(_) => None,
        })
        .unwrap_or_default()
}

fn object_id_of(value: &Value) -> Option<ObjectId> {
    match value {
        Value::Object(id) => Some(*id),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => None,
    }
}

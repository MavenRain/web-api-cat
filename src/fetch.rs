//! `fetch(url)` -- synchronous net-cat call wrapped as a boa-cat
//! `NativeFn` returning a `{ ok, status, statusText, text, headers }`
//! object.
//!
//! v0 limitations:
//!
//! - Plain HTTP only (`https://` returns a thrown `TypeError`).
//! - Synchronous (no Promise / event-loop integration).
//! - `GET` requests only; body / method are deferred until net-cat grows
//!   them in v0.2.

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::Object;
use net_cat::{Method, Request, Response, Url};

/// The `fetch(url)` native callable.
///
/// # Errors
///
/// Never returns `Err`; failed fetches become `Outcome::Throw`.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
pub fn fetch_impl(args: Vec<Value>, _this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let url_arg = match args.first() {
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) | None => None,
    };
    if let Some(url_str) = url_arg {
        match perform_fetch(&url_str) {
            Ok(response) => {
                let (value, heap) = build_response_object(&response, heap);
                Ok((Outcome::Normal(value), heap, fuel))
            }
            Err(err) => Ok((
                Outcome::Throw(Value::String(format!("TypeError: fetch failed: {err}"))),
                heap,
                fuel,
            )),
        }
    } else {
        Ok((
            Outcome::Throw(Value::String(
                "TypeError: fetch() requires a URL string".to_owned(),
            )),
            heap,
            fuel,
        ))
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
    let _ = props.insert("text".to_owned(), Value::String(text));
    let _ = props.insert("headers".to_owned(), headers_value);
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

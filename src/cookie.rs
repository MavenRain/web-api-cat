//! `document.cookie` bridge between the host's cookie jar and JS.
//!
//! v0.4 installs `document.cookie` as a [`boa_cat::AccessorPair`]:
//! reads dispatch through a native getter that returns the current
//! host-supplied projection of the jar; writes dispatch through a
//! native setter that records the raw RHS string (with attributes,
//! e.g. `"name=v; Max-Age=600; Path=/admin"`) into a hidden write
//! log AND updates the JS-visible projection so that subsequent
//! reads within the same script see the just-written cookie.
//!
//! The host's plumbing is unchanged in shape -- it still seeds the
//! visible projection pre-eval and reads what changed post-eval --
//! but the read side now returns the per-write entries (one per
//! `document.cookie = "..."` statement) instead of one bulk string.
//! That lets the host parse each entry with [`cookie::Cookie::parse`]
//! and preserve attribute semantics across multiple writes, which
//! the v3.8 host-side heuristic in tauri-runtime-servocat could only
//! do for a single write at a time.
//!
//! Hidden state lives on the document object under two string
//! properties: [`VISIBLE_KEY`] holds the JS-readable projection,
//! and [`WRITES_KEY`] holds a `\n`-separated log of raw RHS strings
//! from every setter invocation.
//!
//! # Examples
//!
//! ```
//! # fn main() -> Result<(), web_api_cat::Error> {
//! use boa_cat::heap::Heap;
//! use web_api_cat::{document, get_document_cookie, set_document_cookie};
//!
//! let html = html_cat::parse("<html><body></body></html>")?;
//! let (document_value, _root, heap) = document::build(&html, Heap::new());
//! let heap = set_document_cookie(&document_value, heap, "session=abc");
//! let snapshot = get_document_cookie(&document_value, &heap);
//! assert_eq!(snapshot.as_deref(), Some("session=abc"));
//! # Ok(())
//! # }
//! ```

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{AccessorPair, Object, ObjectId};

/// Property key under which the user-facing `cookie` accessor lives.
pub const PROPERTY_KEY: &str = "cookie";

/// Hidden property key under which the JS-visible projection of the
/// host's jar lives.  The getter reads it; the setter updates it
/// when JS writes a new `name=value`.
pub const VISIBLE_KEY: &str = "__cookies_visible__";

/// Hidden property key under which the `\n`-separated log of raw
/// setter inputs lives.  The host reads it post-eval and parses each
/// entry with `cookie::Cookie::parse` to preserve attributes.
pub const WRITES_KEY: &str = "__cookie_writes__";

/// Install the v0.4 `document.cookie` accessor on `document_value`.
/// Idempotent: re-running on the same object overwrites the previous
/// hidden state.  Called by [`crate::document::build`] so callers of
/// [`crate::install`] don't need to invoke it manually.
#[must_use]
pub fn install_cookie_accessor(document_value: &Value, heap: Heap) -> Heap {
    let pair =
        object_id_of(document_value).and_then(|id| heap.object(id).cloned().map(|obj| (id, obj)));
    pair.into_iter().fold(heap, |heap, (id, obj)| {
        let accessor = AccessorPair::new(
            Some(Value::Native(cookie_getter_impl)),
            Some(Value::Native(cookie_setter_impl)),
        );
        let updated = obj
            .with(VISIBLE_KEY.to_owned(), Value::String(String::new()))
            .with(WRITES_KEY.to_owned(), Value::String(String::new()))
            .with_accessor(PROPERTY_KEY.to_owned(), accessor);
        heap.store_object(id, updated).unwrap_or_else(|h| h)
    })
}

/// Seed the JS-visible projection on `document_value`.  Hosts call
/// this once per evaluation, before the script runs, with the
/// already-filtered cookie string (e.g. `HttpOnly` cookies removed).
///
/// The previous write log is cleared so post-eval
/// [`read_cookie_writes`] surfaces only the writes from this run.
#[must_use]
pub fn set_document_cookie(document_value: &Value, heap: Heap, cookies: &str) -> Heap {
    let pair =
        object_id_of(document_value).and_then(|id| heap.object(id).cloned().map(|obj| (id, obj)));
    let cookies_owned = cookies.to_owned();
    pair.into_iter().fold(heap, |heap, (id, obj)| {
        let updated = obj
            .with(VISIBLE_KEY.to_owned(), Value::String(cookies_owned.clone()))
            .with(WRITES_KEY.to_owned(), Value::String(String::new()));
        heap.store_object(id, updated).unwrap_or_else(|h| h)
    })
}

/// Read the current JS-visible projection of the cookie jar.
/// Returns `None` when `document_value` is not an object or the
/// hidden slot is missing.  Useful for hosts that want to read the
/// running projection (which the setter updates in place) rather
/// than parse the write log.
#[must_use]
pub fn get_document_cookie(document_value: &Value, heap: &Heap) -> Option<String> {
    read_string_property(document_value, heap, VISIBLE_KEY)
}

/// Read the raw RHS strings JS passed to the `document.cookie`
/// setter during the most recent script run, in write order.  Each
/// entry is a Set-Cookie-style string (e.g. `"name=v; Max-Age=600"`)
/// that the host should parse with `cookie::Cookie::parse` to
/// preserve attribute semantics.  Returns an empty `Vec` when the
/// log is empty or `document_value` isn't an object.
#[must_use]
pub fn read_cookie_writes(document_value: &Value, heap: &Heap) -> Vec<String> {
    read_string_property(document_value, heap, WRITES_KEY)
        .filter(|s| !s.is_empty())
        .map(|s| s.split('\n').map(str::to_owned).collect())
        .unwrap_or_default()
}

fn read_string_property(value: &Value, heap: &Heap, key: &str) -> Option<String> {
    let id = object_id_of(value)?;
    let obj = heap.object(id)?;
    obj.get(key).and_then(|value| match value {
        Value::String(s) => Some(s.clone()),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::Object(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => None,
    })
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

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn cookie_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let visible = read_string_property(&this, &heap, VISIBLE_KEY).unwrap_or_default();
    Ok((Outcome::Normal(Value::String(visible)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn cookie_setter_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let raw = string_arg(&args, 0);
    let outcome_heap = object_id_of(&this)
        .and_then(|id| heap.object(id).cloned().map(|obj| (id, obj)))
        .map_or(heap.clone(), |(id, obj)| {
            let visible = read_string_property(&this, &heap, VISIBLE_KEY).unwrap_or_default();
            let writes = read_string_property(&this, &heap, WRITES_KEY).unwrap_or_default();
            let new_visible = update_visible(&visible, &raw);
            let new_writes = append_write_log(&writes, &raw);
            let updated = update_hidden_state(&obj, new_visible, new_writes);
            heap.clone().store_object(id, updated).unwrap_or_else(|h| h)
        });
    Ok((Outcome::Normal(Value::Undefined), outcome_heap, fuel))
}

fn update_hidden_state(obj: &Object, visible: String, writes: String) -> Object {
    obj.with(VISIBLE_KEY.to_owned(), Value::String(visible))
        .with(WRITES_KEY.to_owned(), Value::String(writes))
}

fn string_arg(args: &[Value], idx: usize) -> String {
    match args.get(idx) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => format!("{n}"),
        Some(Value::Boolean(b)) => format!("{b}"),
        Some(Value::Null) => "null".to_owned(),
        Some(Value::Undefined) | None => String::new(),
        Some(Value::Object(_) | Value::Function(_) | Value::Native(_) | Value::Promise(_)) => {
            "[object]".to_owned()
        }
    }
}

fn append_write_log(existing: &str, entry: &str) -> String {
    if existing.is_empty() {
        entry.to_owned()
    } else {
        format!("{existing}\n{entry}")
    }
}

fn update_visible(visible: &str, raw_write: &str) -> String {
    // Extract just the `name=value` head of the Set-Cookie style RHS
    // (everything before the first `;`).  Attributes after the
    // first `;` are recorded in the write log -- they don't surface
    // in the JS-readable projection per browser semantics.
    let head = raw_write.split(';').next().unwrap_or("").trim();
    parse_name(head).map_or_else(
        || visible.to_owned(),
        |name| {
            let kept = retain_other_entries(visible, name);
            if kept.is_empty() {
                head.to_owned()
            } else {
                format!("{kept}; {head}")
            }
        },
    )
}

fn parse_name(entry: &str) -> Option<&str> {
    entry
        .split_once('=')
        .map(|(name, _value)| name.trim())
        .filter(|name| !name.is_empty())
}

fn retain_other_entries(visible: &str, name_to_drop: &str) -> String {
    visible
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter(|entry| {
            parse_name(entry).is_none_or(|name| !name.eq_ignore_ascii_case(name_to_drop))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

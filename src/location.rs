//! v0.7.22 Location API: `window.location` (also bound globally
//! as `location`).  Exposes the URL via accessor properties
//! (`href` read-write, `protocol` / `host` / `hostname` / `port` /
//! `pathname` / `search` / `hash` / `origin` read-only) and three
//! navigation methods (`assign`, `replace`, `reload`).
//!
//! The Location object owns two hidden state slots:
//!
//! - `__href` -- the current URL string (default empty).
//! - `__pending_nav` -- the most recent navigation intent the
//!   script requested, encoded as `{ kind: "assign" | "replace" |
//!   "reload", url: "..." }`.  Downstream consumers (e.g.
//!   tauri-runtime-servocat) read this via
//!   [`read_pending_navigation`] after script execution to decide
//!   whether to follow the navigation, then call
//!   [`clear_pending_navigation`] to consume the intent.
//!
//! Reading parsed-URL accessors (`protocol`, `host`, etc.) parses
//! the current `__href` on each read via the `url` crate.  Invalid
//! URLs return empty strings rather than throwing, so scripts
//! like `if (location.protocol === 'https:')` don't blow up on
//! an empty initial URL.

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::env::{Binding, Env};
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{AccessorPair, Object, ObjectId};
use url::Url;

/// Key for the `__href` hidden slot on the Location object.
pub const HREF_SLOT: &str = "__href";
/// Key for the `__pending_nav` hidden slot on the Location object.
pub const PENDING_NAV_SLOT: &str = "__pending_nav";

/// A navigation intent recorded by the script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationIntent {
    /// `location.href = url` or `location.assign(url)`.
    Assign(String),
    /// `location.replace(url)`.
    Replace(String),
    /// `location.reload()`.
    Reload,
}

/// Build the Location object and bind it as `location` (top-level)
/// AND as `window.location` (must be wired by the caller since
/// `window` is built in `install.rs`).  Returns the Location's
/// `Value::Object` plus the updated heap.
#[must_use]
pub fn build_location_object(heap: Heap) -> (Value, Heap) {
    let mut props: BTreeMap<String, Value> = BTreeMap::new();
    let _ = props.insert(HREF_SLOT.to_owned(), Value::String(String::new()));
    let _ = props.insert(PENDING_NAV_SLOT.to_owned(), Value::Null);
    let _ = props.insert("assign".to_owned(), Value::Native(assign_impl));
    let _ = props.insert("replace".to_owned(), Value::Native(replace_impl));
    let _ = props.insert("reload".to_owned(), Value::Native(reload_impl));
    let _ = props.insert("toString".to_owned(), Value::Native(to_string_impl));
    let (id, heap) = heap.alloc_object(Object::from_properties(props));
    let value = Value::Object(id);
    let heap = install_accessor_pair(&value, "href", href_getter, href_setter, heap);
    let heap = install_accessor_pair(&value, "protocol", protocol_getter, noop_setter, heap);
    let heap = install_accessor_pair(&value, "host", host_getter, noop_setter, heap);
    let heap = install_accessor_pair(&value, "hostname", hostname_getter, noop_setter, heap);
    let heap = install_accessor_pair(&value, "port", port_getter, noop_setter, heap);
    let heap = install_accessor_pair(&value, "pathname", pathname_getter, noop_setter, heap);
    let heap = install_accessor_pair(&value, "search", search_getter, noop_setter, heap);
    let heap = install_accessor_pair(&value, "hash", hash_getter, noop_setter, heap);
    let heap = install_accessor_pair(&value, "origin", origin_getter, noop_setter, heap);
    (value, heap)
}

/// v0.7.22 helper for downstream consumers: set the current
/// `location.href` to `href`.  Downstream code typically calls
/// this right after [`crate::install`] to seed the URL.  Updates
/// the Location object's `__href` slot in place.
#[must_use]
pub fn set_location_href(env: &Env, href: &str, heap: Heap) -> Heap {
    let Some(location_id) = location_object_id(env, &heap) else {
        return heap;
    };
    let Some(object) = heap.object(location_id).cloned() else {
        return heap;
    };
    let updated = object.with(HREF_SLOT.to_owned(), Value::String(href.to_owned()));
    heap.store_object(location_id, updated)
        .unwrap_or_else(|h| h)
}

/// v0.7.22 helper for downstream consumers: read the most recent
/// navigation intent the script requested.  Returns `None` if no
/// nav was requested or `location` is unbound.
#[must_use]
pub fn read_pending_navigation(env: &Env, heap: &Heap) -> Option<NavigationIntent> {
    let location_id = location_object_id(env, heap)?;
    let object = heap.object(location_id)?;
    decode_pending_nav(object.get(PENDING_NAV_SLOT)?)
}

/// v0.7.22 helper for downstream consumers: clear the pending
/// navigation intent.  Call after acting on
/// [`read_pending_navigation`] so the next script run starts
/// from a clean slate.
#[must_use]
pub fn clear_pending_navigation(env: &Env, heap: Heap) -> Heap {
    let Some(location_id) = location_object_id(env, &heap) else {
        return heap;
    };
    let Some(object) = heap.object(location_id).cloned() else {
        return heap;
    };
    let updated = object.with(PENDING_NAV_SLOT.to_owned(), Value::Null);
    heap.store_object(location_id, updated)
        .unwrap_or_else(|h| h)
}

fn location_object_id(env: &Env, heap: &Heap) -> Option<ObjectId> {
    let binding = env.lookup("location")?;
    let value = match binding {
        Binding::Cell(cell_id) => heap.cell(*cell_id)?.value().clone(),
        Binding::Direct(v) => v.clone(),
    };
    object_id_of(&value)
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

fn decode_pending_nav(value: &Value) -> Option<NavigationIntent> {
    match value {
        Value::String(s) => decode_pending_nav_string(s),
        Value::Null
        | Value::Undefined
        | Value::Object(_)
        | Value::Number(_)
        | Value::Boolean(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => None,
    }
}

fn decode_pending_nav_string(encoded: &str) -> Option<NavigationIntent> {
    let (kind, payload) = encoded.split_once('|').unwrap_or((encoded, ""));
    match kind {
        "assign" => Some(NavigationIntent::Assign(payload.to_owned())),
        "replace" => Some(NavigationIntent::Replace(payload.to_owned())),
        "reload" => Some(NavigationIntent::Reload),
        _other => None,
    }
}

fn encode_pending_nav(intent: &NavigationIntent) -> String {
    match intent {
        NavigationIntent::Assign(url) => format!("assign|{url}"),
        NavigationIntent::Replace(url) => format!("replace|{url}"),
        NavigationIntent::Reload => "reload|".to_owned(),
    }
}

#[must_use]
fn install_accessor_pair(
    object_value: &Value,
    key: &str,
    getter: fn(Vec<Value>, Value, Heap, Fuel) -> EvalResult,
    setter: fn(Vec<Value>, Value, Heap, Fuel) -> EvalResult,
    heap: Heap,
) -> Heap {
    let Some(object_id) = object_id_of(object_value) else {
        return heap;
    };
    let Some(object) = heap.object(object_id).cloned() else {
        return heap;
    };
    let accessor = AccessorPair::new(Some(Value::Native(getter)), Some(Value::Native(setter)));
    let updated = object.with_accessor(key.to_owned(), accessor);
    heap.store_object(object_id, updated).unwrap_or_else(|h| h)
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn href_getter(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    Ok((
        Outcome::Normal(Value::String(href_string(&this, &heap))),
        heap,
        fuel,
    ))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn href_setter(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let url = stringify_first_arg(&args);
    let heap = write_href(&this, &url, heap);
    let heap = write_pending_nav(&this, &NavigationIntent::Assign(url), heap);
    Ok((Outcome::Normal(Value::Undefined), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn protocol_getter(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = parsed_field(&this, &heap, |url| format!("{}:", url.scheme()));
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn host_getter(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = parsed_field(&this, &heap, |url| {
        url.port().map_or_else(
            || url.host_str().unwrap_or_default().to_owned(),
            |port| format!("{}:{port}", url.host_str().unwrap_or_default()),
        )
    });
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn hostname_getter(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = parsed_field(&this, &heap, |url| {
        url.host_str().unwrap_or_default().to_owned()
    });
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn port_getter(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = parsed_field(&this, &heap, |url| {
        url.port().map_or_else(String::new, |p| format!("{p}"))
    });
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn pathname_getter(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = parsed_field(&this, &heap, |url| url.path().to_owned());
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn search_getter(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = parsed_field(&this, &heap, |url| {
        url.query().map_or_else(String::new, |q| format!("?{q}"))
    });
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn hash_getter(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = parsed_field(&this, &heap, |url| {
        url.fragment().map_or_else(String::new, |f| format!("#{f}"))
    });
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn origin_getter(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = parsed_field(&this, &heap, |url| {
        let host = url.host_str().unwrap_or_default();
        if host.is_empty() {
            String::new()
        } else {
            url.port().map_or_else(
                || format!("{}://{host}", url.scheme()),
                |port| format!("{}://{host}:{port}", url.scheme()),
            )
        }
    });
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn assign_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let url = stringify_first_arg(&args);
    let heap = write_href(&this, &url, heap);
    let heap = write_pending_nav(&this, &NavigationIntent::Assign(url), heap);
    Ok((Outcome::Normal(Value::Undefined), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn replace_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let url = stringify_first_arg(&args);
    let heap = write_href(&this, &url, heap);
    let heap = write_pending_nav(&this, &NavigationIntent::Replace(url), heap);
    Ok((Outcome::Normal(Value::Undefined), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn reload_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let heap = write_pending_nav(&this, &NavigationIntent::Reload, heap);
    Ok((Outcome::Normal(Value::Undefined), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn to_string_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    Ok((
        Outcome::Normal(Value::String(href_string(&this, &heap))),
        heap,
        fuel,
    ))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn noop_setter(_args: Vec<Value>, _this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    Ok((Outcome::Normal(Value::Undefined), heap, fuel))
}

fn href_string(this: &Value, heap: &Heap) -> String {
    object_id_of(this)
        .and_then(|id| heap.object(id))
        .and_then(|object| match object.get(HREF_SLOT) {
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) | None => None,
        })
        .unwrap_or_default()
}

fn parsed_field<F: FnOnce(&Url) -> String>(this: &Value, heap: &Heap, f: F) -> String {
    let href = href_string(this, heap);
    Url::parse(&href)
        .ok()
        .map(|url| f(&url))
        .unwrap_or_default()
}

fn write_href(this: &Value, href: &str, heap: Heap) -> Heap {
    let Some(object_id) = object_id_of(this) else {
        return heap;
    };
    let Some(object) = heap.object(object_id).cloned() else {
        return heap;
    };
    let updated = object.with(HREF_SLOT.to_owned(), Value::String(href.to_owned()));
    heap.store_object(object_id, updated).unwrap_or_else(|h| h)
}

fn write_pending_nav(this: &Value, intent: &NavigationIntent, heap: Heap) -> Heap {
    let Some(object_id) = object_id_of(this) else {
        return heap;
    };
    let Some(object) = heap.object(object_id).cloned() else {
        return heap;
    };
    let updated = object.with(
        PENDING_NAV_SLOT.to_owned(),
        Value::String(encode_pending_nav(intent)),
    );
    heap.store_object(object_id, updated).unwrap_or_else(|h| h)
}

fn stringify_first_arg(args: &[Value]) -> String {
    match args.first() {
        Some(Value::String(s)) => s.clone(),
        Some(other) => format!("{other}"),
        None => String::new(),
    }
}

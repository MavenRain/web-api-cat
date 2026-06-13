//! `localStorage` / `sessionStorage` bridge between the host's
//! storage projection and JS.
//!
//! v0.7.0 ships both storages as `Storage`-shaped Objects exposing
//! `getItem(key)`, `setItem(key, value)`, `removeItem(key)`,
//! `clear()`, `key(index)`, and a read-only `length` accessor.
//! Each Storage Object owns a hidden `__items__` Object holding
//! the actual key->value pairs (always coerced to string per
//! spec).
//!
//! The host wires the JS-visible projection in two steps, mirror
//! of how `document.cookie` works:
//!
//!   1. Pre-eval: [`seed_storage`] seeds `__items__` from the
//!      host's persisted state.
//!   2. Post-eval: [`read_storage_items`] reads the final
//!      key/value pairs so the host can persist any
//!      `setItem` / `removeItem` / `clear` changes.
//!
//! Iteration order is `BTreeMap`-sorted-by-key.  The Storage spec
//! leaves `key(index)` order implementation-defined, so this is
//! conformant; the deterministic sort also makes the projection
//! easier to test.
//!
//! # Examples
//!
//! ```
//! # fn main() -> Result<(), web_api_cat::Error> {
//! use boa_cat::{env::Env, heap::Heap};
//! use web_api_cat::{install, storage};
//!
//! let html = html_cat::parse("<html><body></body></html>")?;
//! let (_env, heap) = install(Env::empty(), Heap::new(), &html);
//! # let _ = heap;
//! # Ok(())
//! # }
//! ```
//!
//! Hosts pre-seed and post-read via the storage Value returned
//! from [`build_storage_object`] (a per-Storage instance).

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::env::{Binding, Env};
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{AccessorPair, Object, ObjectId};

/// Hidden property key under which the Storage Object's items map
/// lives.  Exposed (for crate-internal use) because the install
/// module wires the seeding through it.
pub const ITEMS_KEY: &str = "__items__";

/// Look up `localStorage` in `env` and return its Value (the
/// Storage-shaped Object [`crate::install::install`] bound).
/// Hosts call this to get the handle they need for
/// [`seed_storage`] and [`read_storage_items`].  Returns `None`
/// if the binding is missing or the cell has been clobbered.
#[must_use]
pub fn lookup_local_storage(env: &Env, heap: &Heap) -> Option<Value> {
    lookup_storage(env, heap, "localStorage")
}

/// Mirror of [`lookup_local_storage`] for `sessionStorage`.
#[must_use]
pub fn lookup_session_storage(env: &Env, heap: &Heap) -> Option<Value> {
    lookup_storage(env, heap, "sessionStorage")
}

fn lookup_storage(env: &Env, heap: &Heap, name: &str) -> Option<Value> {
    env.lookup(name).and_then(|binding| match binding {
        Binding::Cell(cell_id) => heap.cell(*cell_id).map(|cell| cell.value().clone()),
        Binding::Direct(value) => Some(value.clone()),
    })
}

/// Build a Storage-shaped Object on `heap`.  Used twice by
/// [`crate::install::install`] (once for `localStorage`, once for
/// `sessionStorage`).  The returned Value can be passed back to
/// [`seed_storage`] / [`read_storage_items`] by the host.
#[must_use]
pub fn build_storage_object(heap: Heap) -> (Value, Heap) {
    let (items_id, heap) = heap.alloc_object(Object::from_properties(BTreeMap::new()));
    let mut props = BTreeMap::new();
    let _ = props.insert(ITEMS_KEY.to_owned(), Value::Object(items_id));
    let _ = props.insert("getItem".to_owned(), Value::Native(get_item_impl));
    let _ = props.insert("setItem".to_owned(), Value::Native(set_item_impl));
    let _ = props.insert("removeItem".to_owned(), Value::Native(remove_item_impl));
    let _ = props.insert("clear".to_owned(), Value::Native(clear_impl));
    let _ = props.insert("key".to_owned(), Value::Native(key_impl));
    let (id, heap) = heap.alloc_object(Object::from_properties(props));
    let storage_value = Value::Object(id);
    let heap = install_length_accessor(&storage_value, heap);
    (storage_value, heap)
}

/// Seed the Storage Object's `__items__` Object with `entries`,
/// overwriting any previous state.  Hosts call this once per
/// evaluation, before the script runs, with whatever the user has
/// persisted to disk (or whatever the per-session jar is for
/// `sessionStorage`).
#[must_use]
pub fn seed_storage(storage_value: &Value, heap: Heap, entries: &[(String, String)]) -> Heap {
    let Some(items_id) = items_id_of(storage_value, &heap) else {
        return heap;
    };
    let new_items = entries
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    heap.store_object(items_id, Object::from_properties(new_items))
        .unwrap_or_else(|h| h)
}

/// Read every `(key, value)` pair in `storage_value`'s `__items__`
/// Object, in `BTreeMap`-sorted-by-key order.  Hosts call this
/// post-eval to pick up `setItem` / `removeItem` / `clear` changes.
#[must_use]
pub fn read_storage_items(storage_value: &Value, heap: &Heap) -> Vec<(String, String)> {
    let Some(items_id) = items_id_of(storage_value, heap) else {
        return Vec::new();
    };
    let Some(items) = heap.object(items_id) else {
        return Vec::new();
    };
    items
        .properties()
        .iter()
        .filter_map(|(k, v)| match v {
            Value::String(s) => Some((k.clone(), s.clone())),
            Value::Undefined
            | Value::Null
            | Value::Boolean(_)
            | Value::Number(_)
            | Value::Object(_)
            | Value::Function(_)
            | Value::Native(_)
            | Value::Promise(_) => None,
        })
        .collect()
}

fn install_length_accessor(storage_value: &Value, heap: Heap) -> Heap {
    let Some(storage_id) = object_id_of(storage_value) else {
        return heap;
    };
    let Some(storage) = heap.object(storage_id).cloned() else {
        return heap;
    };
    let pair = AccessorPair::new(Some(Value::Native(length_getter_impl)), None);
    let updated = storage.with_accessor("length".to_owned(), pair);
    heap.store_object(storage_id, updated).unwrap_or_else(|h| h)
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn length_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let count = items_id_of(&this, &heap)
        .and_then(|id| heap.object(id))
        .map_or(0, |items| items.properties().len());
    let count_u32 = u32::try_from(count).unwrap_or(u32::MAX);
    Ok((
        Outcome::Normal(Value::Number(f64::from(count_u32))),
        heap,
        fuel,
    ))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn get_item_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let key = string_arg(&args, 0);
    let value = read_item(&this, &key, &heap).map_or(Value::Null, Value::String);
    Ok((Outcome::Normal(value), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn set_item_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let key = string_arg(&args, 0);
    let value = string_arg(&args, 1);
    let new_heap = write_item(&this, &key, &value, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn remove_item_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let key = string_arg(&args, 0);
    let new_heap = delete_item(&this, &key, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn clear_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let new_heap = clear_items(&this, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn key_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let index = u32_arg(&args, 0);
    let value = items_id_of(&this, &heap)
        .and_then(|id| heap.object(id))
        .and_then(|items| {
            items
                .properties()
                .keys()
                .nth(usize::try_from(index).unwrap_or(usize::MAX))
                .cloned()
        })
        .map_or(Value::Null, Value::String);
    Ok((Outcome::Normal(value), heap, fuel))
}

fn read_item(this: &Value, key: &str, heap: &Heap) -> Option<String> {
    let items_id = items_id_of(this, heap)?;
    let items = heap.object(items_id)?;
    items.get(key).and_then(|v| match v {
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

fn write_item(this: &Value, key: &str, value: &str, heap: Heap) -> Heap {
    let Some(items_id) = items_id_of(this, &heap) else {
        return heap;
    };
    let Some(items) = heap.object(items_id).cloned() else {
        return heap;
    };
    let updated = items.with(key.to_owned(), Value::String(value.to_owned()));
    heap.store_object(items_id, updated).unwrap_or_else(|h| h)
}

fn delete_item(this: &Value, key: &str, heap: Heap) -> Heap {
    let Some(items_id) = items_id_of(this, &heap) else {
        return heap;
    };
    let Some(items) = heap.object(items_id) else {
        return heap;
    };
    let new_items: BTreeMap<String, Value> = items
        .properties()
        .iter()
        .filter(|(k, _)| k.as_str() != key)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    heap.store_object(items_id, Object::from_properties(new_items))
        .unwrap_or_else(|h| h)
}

fn clear_items(this: &Value, heap: Heap) -> Heap {
    let Some(items_id) = items_id_of(this, &heap) else {
        return heap;
    };
    heap.store_object(items_id, Object::from_properties(BTreeMap::new()))
        .unwrap_or_else(|h| h)
}

fn items_id_of(storage_value: &Value, heap: &Heap) -> Option<ObjectId> {
    let storage_id = object_id_of(storage_value)?;
    let storage = heap.object(storage_id)?;
    storage.get(ITEMS_KEY).and_then(|v| match v {
        Value::Object(id) => Some(*id),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::String(_)
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

fn u32_arg(args: &[Value], idx: usize) -> u32 {
    match args.get(idx) {
        Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let value = *n as u32;
            value
        }
        Some(_) | None => 0,
    }
}

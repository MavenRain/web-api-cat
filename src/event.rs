//! `EventTarget` mixin (v0.7.4): `addEventListener(type, callback)`,
//! `removeEventListener(type, callback)`, `dispatchEvent(event)`.
//!
//! Per-element state lives under a hidden `__listeners__` Object
//! shaped as `{ <type>: { 0: cb, 1: cb, ..., length: n }, ... }`
//! -- one type-keyed entry per event type, each holding an
//! array-shaped Object of callbacks in registration order.  We
//! create the slot lazily on first `addEventListener` call so
//! elements that never listen carry no extra heap weight.
//!
//! `dispatchEvent(event)` walks the bubble chain via the v0.6.8
//! `__parent__` backref (no separate event-flow infrastructure
//! needed): the target's own listeners fire first, then each
//! ancestor's in order up to the document root or the first
//! null-parent.  `boa_cat::expression::call_function` (made `pub`
//! in boa-cat 0.7.1) dispatches each callback with `this = level`
//! and `args = [event]`.  Listener throws are intentionally
//! swallowed at the dispatch boundary -- per the DOM spec, a
//! listener's exception is reported to the console but does NOT
//! abort the remaining listeners or the bubble chain.
//!
//! v0 limitations:
//!
//! - No capture phase (bubble only).  `addEventListener(_, _,
//!   true)` ignores the capture flag.
//! - No `Event` constructor; scripts pass plain `{ type: 'foo' }`
//!   objects.  A future chunk can add `new Event(type)` once the
//!   engine grows `NewExpression` dispatch on `NativeFn`
//!   constructors.
//! - No `event.preventDefault()` / `stopPropagation()` -- the
//!   event Object is shuttled through dispatch unchanged.
//! - No `once` / `passive` / `signal` listener options.

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{Object, ObjectId};

/// Hidden property key under which an element's listener map
/// lives once it has any registered listeners.
pub const LISTENERS_KEY: &str = "__listeners__";

/// `EventTarget.addEventListener(type, callback)` (v0.7.4): append
/// `callback` to the listener queue for `type` on `this`.  Lazy:
/// creates the `__listeners__` Object and the per-type array on
/// first use.  Duplicate `(type, callback)` pairs ARE inserted
/// (matches the spec only when the second arg differs by
/// reference; we keep all dupes for simplicity).  Third
/// `options` / `useCapture` arg is currently ignored.
///
/// # Errors
///
/// Never returns `Err`; bad inputs no-op.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn add_event_listener_impl(
    args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let event_type = string_arg(&args, 0);
    let callback = args.get(1).cloned().unwrap_or(Value::Undefined);
    let new_heap = append_listener(&this, &event_type, callback, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

/// `EventTarget.removeEventListener(type, callback)` (v0.7.4):
/// drop every queue entry whose Value equals `callback` (via
/// `Value::PartialEq` -- `Value::Function(id)` equality is by
/// `FunctionId`; `Value::Native(fn_ptr)` equality is by function
/// pointer).  No-op when no matching entry exists.
///
/// # Errors
///
/// Never returns `Err`; bad inputs no-op.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn remove_event_listener_impl(
    args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let event_type = string_arg(&args, 0);
    let callback = args.get(1).cloned().unwrap_or(Value::Undefined);
    let new_heap = drop_listener(&this, &event_type, &callback, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

/// `EventTarget.dispatchEvent(event)` (v0.7.4): walk the bubble
/// chain (target then ancestors via `__parent__`) and invoke
/// every listener registered for `event.type` at each level.
/// Listener throws are swallowed per DOM dispatch semantics
/// (report-and-continue).  Returns `true` per spec (we don't yet
/// honour `event.defaultPrevented`).
///
/// # Errors
///
/// Returns `Err` only when an underlying `call_function`
/// invocation hits a non-throw engine error (e.g. fuel
/// exhaustion).
#[allow(clippy::needless_pass_by_value)]
pub fn dispatch_event_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let event = args.first().cloned().unwrap_or(Value::Undefined);
    let event_type = read_event_type(&event, &heap);
    let chain = build_bubble_chain(&this, &heap);
    let (heap, fuel) = chain.iter().try_fold((heap, fuel), |(heap, fuel), level| {
        invoke_level_listeners(level, &event, &event_type, heap, fuel)
    })?;
    Ok((Outcome::Normal(Value::Boolean(true)), heap, fuel))
}

fn append_listener(this: &Value, event_type: &str, callback: Value, heap: Heap) -> Heap {
    let Some(element_id) = object_id_of(this) else {
        return heap;
    };
    let Some(element) = heap.object(element_id).cloned() else {
        return heap;
    };
    let (listeners_id, heap) = resolve_or_create_listeners_map(element_id, &element, heap);
    let Some(listeners) = heap.object(listeners_id).cloned() else {
        return heap;
    };
    let (array_id, heap) = resolve_or_create_type_array(listeners_id, &listeners, event_type, heap);
    let Some(array) = heap.object(array_id).cloned() else {
        return heap;
    };
    let length = array_length(&array);
    let updated = array
        .with(format!("{length}"), callback)
        .with("length".to_owned(), Value::Number(f64::from(length + 1)));
    heap.store_object(array_id, updated).unwrap_or_else(|h| h)
}

fn drop_listener(this: &Value, event_type: &str, callback: &Value, heap: Heap) -> Heap {
    let Some(element_id) = object_id_of(this) else {
        return heap;
    };
    let Some(element) = heap.object(element_id) else {
        return heap;
    };
    let Some(listeners_id) = element.get(LISTENERS_KEY).and_then(object_id_from_value) else {
        return heap;
    };
    let Some(listeners) = heap.object(listeners_id) else {
        return heap;
    };
    let Some(array_id) = listeners.get(event_type).and_then(object_id_from_value) else {
        return heap;
    };
    let Some(array) = heap.object(array_id).cloned() else {
        return heap;
    };
    let length = array_length(&array);
    let remaining: Vec<Value> = (0..length)
        .filter_map(|i| array.get(&format!("{i}")).cloned())
        .filter(|v| v != callback)
        .collect();
    let new_length = u32::try_from(remaining.len()).unwrap_or(u32::MAX);
    let pairs: BTreeMap<String, Value> = remaining
        .into_iter()
        .enumerate()
        .map(|(i, v)| (format!("{i}"), v))
        .chain(std::iter::once((
            "length".to_owned(),
            Value::Number(f64::from(new_length)),
        )))
        .collect();
    heap.store_object(array_id, Object::from_properties(pairs))
        .unwrap_or_else(|h| h)
}

fn invoke_level_listeners(
    level: &Value,
    event: &Value,
    event_type: &str,
    heap: Heap,
    fuel: Fuel,
) -> Result<(Heap, Fuel), boa_cat::Error> {
    let listeners = collect_listeners(level, event_type, &heap);
    listeners
        .into_iter()
        .try_fold((heap, fuel), |(heap, fuel), callback| {
            let (_outcome, heap, fuel) = boa_cat::expression::call_function(
                &callback,
                level,
                vec![event.clone()],
                heap,
                fuel,
            )?;
            Ok((heap, fuel))
        })
}

fn collect_listeners(level: &Value, event_type: &str, heap: &Heap) -> Vec<Value> {
    let Some(element_id) = object_id_of(level) else {
        return Vec::new();
    };
    let Some(element) = heap.object(element_id) else {
        return Vec::new();
    };
    let Some(listeners_id) = element.get(LISTENERS_KEY).and_then(object_id_from_value) else {
        return Vec::new();
    };
    let Some(listeners) = heap.object(listeners_id) else {
        return Vec::new();
    };
    let Some(array_id) = listeners.get(event_type).and_then(object_id_from_value) else {
        return Vec::new();
    };
    let Some(array) = heap.object(array_id) else {
        return Vec::new();
    };
    let length = array_length(array);
    (0..length)
        .filter_map(|i| array.get(&format!("{i}")).cloned())
        .collect()
}

fn build_bubble_chain(target: &Value, heap: &Heap) -> Vec<Value> {
    std::iter::successors(Some(target.clone()), |current| read_parent(current, heap)).collect()
}

fn read_parent(value: &Value, heap: &Heap) -> Option<Value> {
    let id = object_id_of(value)?;
    let obj = heap.object(id)?;
    obj.get("__parent__").and_then(|v| match v {
        Value::Object(_) => Some(v.clone()),
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

fn resolve_or_create_listeners_map(
    element_id: ObjectId,
    element: &Object,
    heap: Heap,
) -> (ObjectId, Heap) {
    if let Some(id) = element.get(LISTENERS_KEY).and_then(object_id_from_value) {
        (id, heap)
    } else {
        let (id, heap) = heap.alloc_object(Object::from_properties(BTreeMap::new()));
        let updated = element
            .clone()
            .with(LISTENERS_KEY.to_owned(), Value::Object(id));
        let heap = heap.store_object(element_id, updated).unwrap_or_else(|h| h);
        (id, heap)
    }
}

fn resolve_or_create_type_array(
    listeners_id: ObjectId,
    listeners: &Object,
    event_type: &str,
    heap: Heap,
) -> (ObjectId, Heap) {
    if let Some(id) = listeners.get(event_type).and_then(object_id_from_value) {
        (id, heap)
    } else {
        let empty = Object::from_properties(
            std::iter::once(("length".to_owned(), Value::Number(0.0))).collect(),
        );
        let (id, heap) = heap.alloc_object(empty);
        let updated = listeners
            .clone()
            .with(event_type.to_owned(), Value::Object(id));
        let heap = heap
            .store_object(listeners_id, updated)
            .unwrap_or_else(|h| h);
        (id, heap)
    }
}

fn read_event_type(event: &Value, heap: &Heap) -> String {
    object_id_of(event)
        .and_then(|id| heap.object(id))
        .and_then(|obj| obj.get("type").cloned())
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

fn array_length(array: &Object) -> u32 {
    match array.get("length") {
        Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let length = *n as u32;
            length
        }
        Some(_) | None => 0,
    }
}

fn object_id_of(value: &Value) -> Option<ObjectId> {
    object_id_from_value(value)
}

fn object_id_from_value(value: &Value) -> Option<ObjectId> {
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

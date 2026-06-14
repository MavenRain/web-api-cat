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
//! - No `Event` constructor; scripts pass plain `{ type: 'foo' }`
//!   objects.  v0.7.5 augments the supplied event with
//!   `target` / `currentTarget` / `defaultPrevented` /
//!   `preventDefault` / `stopPropagation` /
//!   `stopImmediatePropagation`, so scripts written against the
//!   spec's Event API work without needing the constructor.  A
//!   future chunk can add `new Event(type)` (boa-cat 0.7.2's
//!   `tests/natives.rs` proved `new SomeNativeFn(args)` already
//!   works through the engine; the surface is unblocked now).
//! - No `once` / `passive` / `signal` listener options.
//!
//! v0.7.6 adds capture-phase support: `addEventListener` accepts a
//! third arg (`useCapture: boolean` or `{ capture: boolean }`).
//! `dispatchEvent` walks three phases per the DOM spec -- CAPTURE
//! (root -> target's parent, capture listeners only), `AT_TARGET`
//! (all of target's listeners regardless of phase), BUBBLE
//! (target's parent -> root, bubble listeners only).  Storage shape
//! changed: each per-type slot is an array of `{ callback, capture }`
//! Object entries instead of bare callable Values.

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{Object, ObjectId};

/// Hidden property key under which an element's listener map
/// lives once it has any registered listeners.
pub const LISTENERS_KEY: &str = "__listeners__";

/// `EventTarget.addEventListener(type, callback, options)`
/// (v0.7.4, capture in v0.7.6): append `callback` to the listener
/// queue for `type` on `this`, tagged with the capture flag
/// extracted from `options`.  Lazy: creates the `__listeners__`
/// Object and the per-type array on first use.  Third arg may be
/// a `Value::Boolean` (interpreted as `useCapture`) or an
/// `Value::Object` with a `capture` property; anything else
/// defaults to `capture = false`.  Other options keys
/// (`once`/`passive`/`signal`) are ignored.
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
    let capture = parse_capture_arg(&args, &heap);
    let new_heap = append_listener(&this, &event_type, callback, capture, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

/// `EventTarget.removeEventListener(type, callback, options)`
/// (v0.7.4, capture in v0.7.6): drop queue entries whose
/// `(callback, capture)` pair matches.  Per DOM spec, a listener
/// added with `useCapture = true` is NOT removed by a call with
/// `useCapture = false` and vice versa; the third arg is parsed
/// the same way `addEventListener` parses it.
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
    let capture = parse_capture_arg(&args, &heap);
    let new_heap = drop_listener(&this, &event_type, &callback, capture, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

/// `EventTarget.dispatchEvent(event)` (v0.7.4, extended v0.7.5):
/// walk the bubble chain (target then ancestors via
/// `__parent__`) and invoke every listener registered for
/// `event.type` at each level.  v0.7.5 decorates the supplied
/// event with `target` / `currentTarget` / `defaultPrevented`
/// (initially `false`) and `preventDefault` /
/// `stopPropagation` / `stopImmediatePropagation` methods so
/// listeners can interact with the dispatch in spec-compliant
/// shape.  `currentTarget` updates per bubble level;
/// `stopPropagation` halts the bubble after the current level
/// finishes; `stopImmediatePropagation` halts both remaining
/// listeners at the current level AND the bubble; `preventDefault`
/// sets `defaultPrevented = true` and makes this fn return
/// `false`.  Listener throws are swallowed per DOM dispatch
/// semantics (report-and-continue).
///
/// # Errors
///
/// Returns `Err` only when an underlying `call_function`
/// invocation hits a non-throw engine error (e.g. fuel
/// exhaustion).
#[allow(clippy::needless_pass_by_value)]
pub fn dispatch_event_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let event = args.first().cloned().unwrap_or(Value::Undefined);
    let Some(target_id) = object_id_of(&this) else {
        return Ok((Outcome::Normal(Value::Boolean(true)), heap, fuel));
    };
    // v0.7.9: decorate the user's event Object in place so post-
    // dispatch reads of `defaultPrevented` etc. on the caller's
    // reference reflect what happened during the bubble walk.
    // Plain `{type: 'foo'}` objects gain the spec slots; properly-
    // constructed `new Event(type)` objects get their stop / target
    // slots reset for this dispatch.
    let heap = decorate_event_in_place(&event, target_id, heap);
    let decorated = event;
    let event_type = read_event_type(&decorated, &heap);
    let chain = build_bubble_chain(&this, &heap);
    let ancestors: Vec<Value> = chain.iter().skip(1).cloned().collect();
    let capture_chain: Vec<Value> = ancestors.iter().rev().cloned().collect();
    // CAPTURE phase: walk ancestors root -> target's parent, fire
    // capture-tagged listeners only.
    let (heap, fuel) = walk_chain(
        &capture_chain,
        &decorated,
        &event_type,
        ListenerFilter::Capture,
        heap,
        fuel,
    )?;
    // AT_TARGET phase: fire all of target's listeners in
    // registration order regardless of phase tag.
    let (heap, fuel) = if read_bool_flag(&decorated, PROPAGATION_STOPPED_KEY, &heap)
        || read_bool_flag(&decorated, IMMEDIATE_STOPPED_KEY, &heap)
    {
        (heap, fuel)
    } else {
        let heap = set_current_target(&decorated, target_id, heap);
        invoke_level_listeners(
            &this,
            &decorated,
            &event_type,
            ListenerFilter::Any,
            heap,
            fuel,
        )?
    };
    // BUBBLE phase: walk ancestors target's parent -> root, fire
    // non-capture listeners only.
    let (heap, fuel) = walk_chain(
        &ancestors,
        &decorated,
        &event_type,
        ListenerFilter::Bubble,
        heap,
        fuel,
    )?;
    let default_prevented = read_bool_flag(&decorated, DEFAULT_PREVENTED_KEY, &heap);
    Ok((
        Outcome::Normal(Value::Boolean(!default_prevented)),
        heap,
        fuel,
    ))
}

#[derive(Clone, Copy)]
enum ListenerFilter {
    Capture,
    Bubble,
    Any,
}

fn walk_chain(
    chain: &[Value],
    event: &Value,
    event_type: &str,
    filter: ListenerFilter,
    heap: Heap,
    fuel: Fuel,
) -> Result<(Heap, Fuel), boa_cat::Error> {
    chain.iter().try_fold((heap, fuel), |(heap, fuel), level| {
        if read_bool_flag(event, PROPAGATION_STOPPED_KEY, &heap)
            || read_bool_flag(event, IMMEDIATE_STOPPED_KEY, &heap)
        {
            Ok((heap, fuel))
        } else {
            let heap = if let Some(id) = object_id_of(level) {
                set_current_target(event, id, heap)
            } else {
                heap
            };
            invoke_level_listeners(level, event, event_type, filter, heap, fuel)
        }
    })
}

/// Property key under which the `event.defaultPrevented` flag
/// lives once dispatchEvent has decorated the event.
pub const DEFAULT_PREVENTED_KEY: &str = "defaultPrevented";

const PROPAGATION_STOPPED_KEY: &str = "__propagation_stopped__";
const IMMEDIATE_STOPPED_KEY: &str = "__immediate_propagation_stopped__";

/// v0.7.5 `event.preventDefault()` impl: set
/// `this.defaultPrevented = true`.  No-op if `this` isn't an
/// Object.  Idempotent.
///
/// v0.7.10 honours the spec's `cancelable` gate: preventDefault
/// only flips the flag when `this.cancelable === true`.  Plain
/// object literals (no `cancelable` key) and
/// `new Event(type)` constructions without `{ cancelable: true }`
/// land on the false path -- calling preventDefault is a silent
/// no-op, and `dispatchEvent` returns `true`.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn prevent_default_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let new_heap = if read_bool_flag(&this, "cancelable", &heap) {
        set_bool_flag(&this, DEFAULT_PREVENTED_KEY, heap)
    } else {
        heap
    };
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

/// v0.7.5 `event.stopPropagation()` impl: set the hidden
/// propagation-stopped flag so the bubble walk halts after the
/// current level finishes.  Remaining listeners at the current
/// level still fire (see `stopImmediatePropagation` for the
/// stricter variant).
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn stop_propagation_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let new_heap = set_bool_flag(&this, PROPAGATION_STOPPED_KEY, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

/// v0.7.5 `event.stopImmediatePropagation()` impl: set both the
/// propagation-stopped flag AND the immediate-stopped flag so
/// remaining listeners at the current level are skipped in
/// addition to the bubble halt.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn stop_immediate_propagation_impl(
    _args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let heap = set_bool_flag(&this, PROPAGATION_STOPPED_KEY, heap);
    let heap = set_bool_flag(&this, IMMEDIATE_STOPPED_KEY, heap);
    Ok((Outcome::Normal(Value::Undefined), heap, fuel))
}

fn decorate_event_in_place(event: &Value, target_id: ObjectId, heap: Heap) -> Heap {
    let Some(event_id) = object_id_of(event) else {
        return heap;
    };
    let Some(obj) = heap.object(event_id).cloned() else {
        return heap;
    };
    let updated = obj
        .with("target".to_owned(), Value::Object(target_id))
        .with("currentTarget".to_owned(), Value::Object(target_id))
        .with(DEFAULT_PREVENTED_KEY.to_owned(), Value::Boolean(false))
        .with(PROPAGATION_STOPPED_KEY.to_owned(), Value::Boolean(false))
        .with(IMMEDIATE_STOPPED_KEY.to_owned(), Value::Boolean(false))
        .with(
            "preventDefault".to_owned(),
            Value::Native(prevent_default_impl),
        )
        .with(
            "stopPropagation".to_owned(),
            Value::Native(stop_propagation_impl),
        )
        .with(
            "stopImmediatePropagation".to_owned(),
            Value::Native(stop_immediate_propagation_impl),
        );
    heap.store_object(event_id, updated).unwrap_or_else(|h| h)
}

/// `new Event(type, options)` constructor (v0.7.9).  Allocates a
/// fresh Event-shaped Object with `type` from `args[0]`,
/// `bubbles` / `cancelable` / `composed` from the optional
/// `options` Object (each defaults to `false`), `defaultPrevented`
/// false, `target` and `currentTarget` null, and the three
/// method bindings (`preventDefault`, `stopPropagation`,
/// `stopImmediatePropagation`).  Also bound as a global, so plain
/// `Event('click')` call form yields the same Object as
/// `new Event('click')` -- boa-cat's `construct` discards the
/// engine-allocated `this` when the `NativeFn` returns an Object.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn event_constructor_impl(
    args: Vec<Value>,
    _this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let event_type = string_arg(&args, 0);
    let options = args.get(1).cloned().unwrap_or(Value::Undefined);
    let (bubbles, cancelable, composed) = parse_event_options(&options, &heap);
    let (event_value, heap) = build_event_object_value(
        &event_type,
        bubbles,
        cancelable,
        composed,
        Value::Null,
        heap,
    );
    Ok((Outcome::Normal(event_value), heap, fuel))
}

/// `new CustomEvent(type, options)` constructor (v0.7.9).
/// Same shape as [`event_constructor_impl`] but reads the
/// `detail` field from `options` (defaults to `null`) and
/// surfaces it on the resulting Object's `detail` slot.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn custom_event_constructor_impl(
    args: Vec<Value>,
    _this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let event_type = string_arg(&args, 0);
    let options = args.get(1).cloned().unwrap_or(Value::Undefined);
    let (bubbles, cancelable, composed) = parse_event_options(&options, &heap);
    let detail = read_options_field(&options, "detail", &heap).unwrap_or(Value::Null);
    let (event_value, heap) =
        build_event_object_value(&event_type, bubbles, cancelable, composed, detail, heap);
    Ok((Outcome::Normal(event_value), heap, fuel))
}

fn build_event_object_value(
    event_type: &str,
    bubbles: bool,
    cancelable: bool,
    composed: bool,
    detail: Value,
    heap: Heap,
) -> (Value, Heap) {
    let mut props = BTreeMap::new();
    let _ = props.insert("type".to_owned(), Value::String(event_type.to_owned()));
    let _ = props.insert("bubbles".to_owned(), Value::Boolean(bubbles));
    let _ = props.insert("cancelable".to_owned(), Value::Boolean(cancelable));
    let _ = props.insert("composed".to_owned(), Value::Boolean(composed));
    let _ = props.insert(DEFAULT_PREVENTED_KEY.to_owned(), Value::Boolean(false));
    let _ = props.insert("target".to_owned(), Value::Null);
    let _ = props.insert("currentTarget".to_owned(), Value::Null);
    let _ = props.insert("detail".to_owned(), detail);
    let _ = props.insert(PROPAGATION_STOPPED_KEY.to_owned(), Value::Boolean(false));
    let _ = props.insert(IMMEDIATE_STOPPED_KEY.to_owned(), Value::Boolean(false));
    let _ = props.insert(
        "preventDefault".to_owned(),
        Value::Native(prevent_default_impl),
    );
    let _ = props.insert(
        "stopPropagation".to_owned(),
        Value::Native(stop_propagation_impl),
    );
    let _ = props.insert(
        "stopImmediatePropagation".to_owned(),
        Value::Native(stop_immediate_propagation_impl),
    );
    let (id, heap) = heap.alloc_object(Object::from_properties(props));
    (Value::Object(id), heap)
}

fn parse_event_options(options: &Value, heap: &Heap) -> (bool, bool, bool) {
    let Some(obj_id) = object_id_of(options) else {
        return (false, false, false);
    };
    let Some(obj) = heap.object(obj_id) else {
        return (false, false, false);
    };
    (
        read_bool_field(obj, "bubbles"),
        read_bool_field(obj, "cancelable"),
        read_bool_field(obj, "composed"),
    )
}

fn read_bool_field(obj: &Object, key: &str) -> bool {
    obj.get(key)
        .and_then(|v| match v {
            Value::Boolean(b) => Some(*b),
            Value::Undefined
            | Value::Null
            | Value::Number(_)
            | Value::String(_)
            | Value::Object(_)
            | Value::Function(_)
            | Value::Native(_)
            | Value::Promise(_) => None,
        })
        .unwrap_or(false)
}

fn read_options_field(options: &Value, key: &str, heap: &Heap) -> Option<Value> {
    let obj_id = object_id_of(options)?;
    let obj = heap.object(obj_id)?;
    obj.get(key).cloned()
}

fn set_current_target(event: &Value, level_id: ObjectId, heap: Heap) -> Heap {
    let Some(event_id) = object_id_of(event) else {
        return heap;
    };
    let Some(obj) = heap.object(event_id).cloned() else {
        return heap;
    };
    let updated = obj.with("currentTarget".to_owned(), Value::Object(level_id));
    heap.store_object(event_id, updated).unwrap_or_else(|h| h)
}

fn set_bool_flag(this: &Value, key: &str, heap: Heap) -> Heap {
    let Some(id) = object_id_of(this) else {
        return heap;
    };
    let Some(obj) = heap.object(id).cloned() else {
        return heap;
    };
    let updated = obj.with(key.to_owned(), Value::Boolean(true));
    heap.store_object(id, updated).unwrap_or_else(|h| h)
}

fn read_bool_flag(value: &Value, key: &str, heap: &Heap) -> bool {
    object_id_of(value)
        .and_then(|id| heap.object(id))
        .and_then(|obj| obj.get(key))
        .and_then(|v| match v {
            Value::Boolean(b) => Some(*b),
            Value::Undefined
            | Value::Null
            | Value::Number(_)
            | Value::String(_)
            | Value::Object(_)
            | Value::Function(_)
            | Value::Native(_)
            | Value::Promise(_) => None,
        })
        .unwrap_or(false)
}

fn append_listener(
    this: &Value,
    event_type: &str,
    callback: Value,
    capture: bool,
    heap: Heap,
) -> Heap {
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
    let (entry_value, heap) = make_listener_entry(callback, capture, heap);
    let Some(array) = heap.object(array_id).cloned() else {
        return heap;
    };
    let length = array_length(&array);
    let updated = array
        .with(format!("{length}"), entry_value)
        .with("length".to_owned(), Value::Number(f64::from(length + 1)));
    heap.store_object(array_id, updated).unwrap_or_else(|h| h)
}

fn drop_listener(
    this: &Value,
    event_type: &str,
    callback: &Value,
    capture: bool,
    heap: Heap,
) -> Heap {
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
        .filter(|entry| {
            let (entry_cb, entry_capture) = unwrap_listener_entry(entry, &heap);
            !(entry_cb == *callback && entry_capture == capture)
        })
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
    filter: ListenerFilter,
    heap: Heap,
    fuel: Fuel,
) -> Result<(Heap, Fuel), boa_cat::Error> {
    let listeners = collect_listeners(level, event_type, &heap);
    listeners
        .into_iter()
        .try_fold((heap, fuel), |(heap, fuel), (callback, capture)| {
            if read_bool_flag(event, IMMEDIATE_STOPPED_KEY, &heap)
                || !filter_matches(filter, capture)
            {
                Ok((heap, fuel))
            } else {
                let (_outcome, heap, fuel) = boa_cat::expression::call_function(
                    &callback,
                    level,
                    vec![event.clone()],
                    heap,
                    fuel,
                )?;
                Ok((heap, fuel))
            }
        })
}

fn filter_matches(filter: ListenerFilter, capture: bool) -> bool {
    match filter {
        ListenerFilter::Capture => capture,
        ListenerFilter::Bubble => !capture,
        ListenerFilter::Any => true,
    }
}

fn collect_listeners(level: &Value, event_type: &str, heap: &Heap) -> Vec<(Value, bool)> {
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
        .map(|entry| unwrap_listener_entry(&entry, heap))
        .collect()
}

fn make_listener_entry(callback: Value, capture: bool, heap: Heap) -> (Value, Heap) {
    let props: BTreeMap<String, Value> = [
        ("callback".to_owned(), callback),
        ("capture".to_owned(), Value::Boolean(capture)),
    ]
    .into_iter()
    .collect();
    let (id, heap) = heap.alloc_object(Object::from_properties(props));
    (Value::Object(id), heap)
}

fn unwrap_listener_entry(entry: &Value, heap: &Heap) -> (Value, bool) {
    let Some(entry_id) = object_id_of(entry) else {
        return (entry.clone(), false);
    };
    let Some(obj) = heap.object(entry_id) else {
        return (entry.clone(), false);
    };
    let callback = obj.get("callback").cloned().unwrap_or(Value::Undefined);
    let capture = obj
        .get("capture")
        .and_then(|v| match v {
            Value::Boolean(b) => Some(*b),
            Value::Undefined
            | Value::Null
            | Value::Number(_)
            | Value::String(_)
            | Value::Object(_)
            | Value::Function(_)
            | Value::Native(_)
            | Value::Promise(_) => None,
        })
        .unwrap_or(false);
    (callback, capture)
}

fn parse_capture_arg(args: &[Value], heap: &Heap) -> bool {
    args.get(2).is_some_and(|v| match v {
        Value::Boolean(b) => *b,
        Value::Object(id) => heap
            .object(*id)
            .and_then(|obj| obj.get("capture"))
            .and_then(|val| match val {
                Value::Boolean(b) => Some(*b),
                Value::Undefined
                | Value::Null
                | Value::Number(_)
                | Value::String(_)
                | Value::Object(_)
                | Value::Function(_)
                | Value::Native(_)
                | Value::Promise(_) => None,
            })
            .unwrap_or(false),
        Value::Undefined
        | Value::Null
        | Value::Number(_)
        | Value::String(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => false,
    })
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

//! v0.7.18 form-control accessors: the `value` and `checked`
//! properties reflect the corresponding HTML attributes through
//! JS-side property access (`input.value`, `input.checked`).
//!
//! Both accessors are installed on every element (not gated by
//! tag), matching boa-cat's accessor-pair model.  For non-form
//! elements the getter returns the empty string / `false`, which
//! is harmless: scripts that probe `.value` on a `<div>` see an
//! empty string rather than throwing.
//!
//! `checked` is reflected as attribute presence: setting
//! `el.checked = true` writes `__attributes.checked = ""`; setting
//! `el.checked = false` removes the attribute entirely.  This
//! keeps the v0.7.18 `:checked` pseudo-class (which matches by
//! attribute presence) consistent with the property setter.

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{AccessorPair, ObjectId};

use crate::element;

/// Property key for the `value` accessor pair.
pub const VALUE_KEY: &str = "value";

/// Property key for the `checked` accessor pair.
pub const CHECKED_KEY: &str = "checked";

/// Install the `value` accessor pair on `element_value`.
/// Idempotent.
#[must_use]
pub fn install_value_accessor(element_value: &Value, heap: Heap) -> Heap {
    install_accessor(
        element_value,
        VALUE_KEY,
        value_getter_impl,
        value_setter_impl,
        heap,
    )
}

/// Install the `checked` accessor pair on `element_value`.
/// Idempotent.
#[must_use]
pub fn install_checked_accessor(element_value: &Value, heap: Heap) -> Heap {
    install_accessor(
        element_value,
        CHECKED_KEY,
        checked_getter_impl,
        checked_setter_impl,
        heap,
    )
}

#[must_use]
fn install_accessor(
    element_value: &Value,
    key: &str,
    getter: fn(Vec<Value>, Value, Heap, Fuel) -> EvalResult,
    setter: fn(Vec<Value>, Value, Heap, Fuel) -> EvalResult,
    heap: Heap,
) -> Heap {
    let Some(element_id) = object_id_from_value(element_value) else {
        return heap;
    };
    let Some(element) = heap.object(element_id).cloned() else {
        return heap;
    };
    let accessor = AccessorPair::new(Some(Value::Native(getter)), Some(Value::Native(setter)));
    let updated = element.with_accessor(key.to_owned(), accessor);
    heap.store_object(element_id, updated).unwrap_or_else(|h| h)
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn value_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = element::read_attribute(&this, VALUE_KEY, &heap).unwrap_or_default();
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn value_setter_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = match args.first() {
        Some(Value::String(s)) => s.clone(),
        Some(other) => format!("{other}"),
        None => String::new(),
    };
    let new_heap = element::write_attribute(&this, VALUE_KEY, &text, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn checked_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let present = element::read_attribute(&this, CHECKED_KEY, &heap).is_some();
    Ok((Outcome::Normal(Value::Boolean(present)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn checked_setter_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let truthy = match args.first() {
        Some(Value::Boolean(b)) => *b,
        Some(Value::Undefined | Value::Null) | None => false,
        Some(Value::Number(n)) => *n != 0.0 && !n.is_nan(),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Object(_) | Value::Function(_) | Value::Native(_) | Value::Promise(_)) => true,
    };
    let new_heap = if truthy {
        element::write_attribute(&this, CHECKED_KEY, "", heap)
    } else {
        element::remove_attribute(&this, CHECKED_KEY, heap)
    };
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
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

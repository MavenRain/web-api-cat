//! v0.7.20 image element accessors: JS-facing properties for
//! `<img>` -- `src`, `alt`, `width`, `height` (attribute
//! reflection) and `naturalWidth`, `naturalHeight`, `complete`
//! (hidden-slot reflection populated by the downstream consumer
//! after the fetch + decode pipeline runs).
//!
//! Accessors are installed on every element (same model as the
//! v0.7.18 form-control accessors), so non-image elements see
//! harmless defaults: empty string for src / alt, 0 for the
//! widths / heights, false for complete.  Read-only properties
//! (naturalWidth, naturalHeight, complete) install a no-op
//! setter so `el.naturalWidth = 0` does not throw.
//!
//! The downstream consumer (tauri-runtime-servocat) walks the DOM
//! finding `<img>` elements, fetches each `src` via net-cat,
//! decodes via an image crate, then calls [`set_natural_size`]
//! and [`set_complete`] (or writes the `__natural_width` /
//! `__natural_height` / `__complete` slots directly) so
//! subsequent JS reads see the loaded state.

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{AccessorPair, ObjectId};

use crate::element;

pub const SRC_KEY: &str = "src";
pub const ALT_KEY: &str = "alt";
pub const WIDTH_KEY: &str = "width";
pub const HEIGHT_KEY: &str = "height";
pub const NATURAL_WIDTH_KEY: &str = "naturalWidth";
pub const NATURAL_HEIGHT_KEY: &str = "naturalHeight";
pub const COMPLETE_KEY: &str = "complete";

/// Hidden slot the natural-width getter reads.  Downstream
/// consumers populate this after decoding.
pub const NATURAL_WIDTH_SLOT: &str = "__natural_width";
/// Hidden slot the natural-height getter reads.
pub const NATURAL_HEIGHT_SLOT: &str = "__natural_height";
/// Hidden slot the complete getter reads.  Downstream consumers
/// flip this to `true` after a successful load.
pub const COMPLETE_SLOT: &str = "__complete";

/// Install the full image-accessor pack on `element_value`
/// (one call covers all seven properties).  Idempotent.
#[must_use]
pub fn install_image_accessors(element_value: &Value, heap: Heap) -> Heap {
    let heap = install_accessor(
        element_value,
        SRC_KEY,
        Value::Native(src_getter_impl),
        Value::Native(src_setter_impl),
        heap,
    );
    let heap = install_accessor(
        element_value,
        ALT_KEY,
        Value::Native(alt_getter_impl),
        Value::Native(alt_setter_impl),
        heap,
    );
    let heap = install_accessor(
        element_value,
        WIDTH_KEY,
        Value::Native(width_getter_impl),
        Value::Native(width_setter_impl),
        heap,
    );
    let heap = install_accessor(
        element_value,
        HEIGHT_KEY,
        Value::Native(height_getter_impl),
        Value::Native(height_setter_impl),
        heap,
    );
    let heap = install_accessor(
        element_value,
        NATURAL_WIDTH_KEY,
        Value::Native(natural_width_getter_impl),
        Value::Native(noop_setter_impl),
        heap,
    );
    let heap = install_accessor(
        element_value,
        NATURAL_HEIGHT_KEY,
        Value::Native(natural_height_getter_impl),
        Value::Native(noop_setter_impl),
        heap,
    );
    install_accessor(
        element_value,
        COMPLETE_KEY,
        Value::Native(complete_getter_impl),
        Value::Native(noop_setter_impl),
        heap,
    )
}

/// Public helper for downstream consumers: write the decoded
/// natural width and height to the element's hidden slots so
/// subsequent `el.naturalWidth` / `el.naturalHeight` reads see
/// the right values.
#[must_use]
pub fn set_natural_size(element_value: &Value, width: u32, height: u32, heap: Heap) -> Heap {
    let heap = write_slot(
        element_value,
        NATURAL_WIDTH_SLOT,
        Value::Number(f64::from(width)),
        heap,
    );
    write_slot(
        element_value,
        NATURAL_HEIGHT_SLOT,
        Value::Number(f64::from(height)),
        heap,
    )
}

/// Public helper for downstream consumers: mark the image as
/// loaded.  Subsequent `el.complete` reads return `true`.
#[must_use]
pub fn set_complete(element_value: &Value, complete: bool, heap: Heap) -> Heap {
    write_slot(element_value, COMPLETE_SLOT, Value::Boolean(complete), heap)
}

#[must_use]
fn install_accessor(
    element_value: &Value,
    key: &str,
    getter: Value,
    setter: Value,
    heap: Heap,
) -> Heap {
    let Some(element_id) = object_id_from_value(element_value) else {
        return heap;
    };
    let Some(element) = heap.object(element_id).cloned() else {
        return heap;
    };
    let accessor = AccessorPair::new(Some(getter), Some(setter));
    let updated = element.with_accessor(key.to_owned(), accessor);
    heap.store_object(element_id, updated).unwrap_or_else(|h| h)
}

fn write_slot(element_value: &Value, slot: &str, value: Value, heap: Heap) -> Heap {
    let Some(element_id) = object_id_from_value(element_value) else {
        return heap;
    };
    let Some(element) = heap.object(element_id).cloned() else {
        return heap;
    };
    let updated = element.with(slot.to_owned(), value);
    heap.store_object(element_id, updated).unwrap_or_else(|h| h)
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn src_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = element::read_attribute(&this, SRC_KEY, &heap).unwrap_or_default();
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn src_setter_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = stringify_first_arg(&args);
    let new_heap = element::write_attribute(&this, SRC_KEY, &text, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn alt_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = element::read_attribute(&this, ALT_KEY, &heap).unwrap_or_default();
    Ok((Outcome::Normal(Value::String(text)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn alt_setter_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = stringify_first_arg(&args);
    let new_heap = element::write_attribute(&this, ALT_KEY, &text, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn width_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let number = number_from_attribute(&this, WIDTH_KEY, &heap);
    Ok((Outcome::Normal(Value::Number(number)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn width_setter_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = stringify_first_arg(&args);
    let new_heap = element::write_attribute(&this, WIDTH_KEY, &text, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn height_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let number = number_from_attribute(&this, HEIGHT_KEY, &heap);
    Ok((Outcome::Normal(Value::Number(number)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn height_setter_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let text = stringify_first_arg(&args);
    let new_heap = element::write_attribute(&this, HEIGHT_KEY, &text, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn natural_width_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let number = number_from_slot(&this, NATURAL_WIDTH_SLOT, &heap);
    Ok((Outcome::Normal(Value::Number(number)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn natural_height_getter_impl(
    _args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let number = number_from_slot(&this, NATURAL_HEIGHT_SLOT, &heap);
    Ok((Outcome::Normal(Value::Number(number)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn complete_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let present = boolean_from_slot(&this, COMPLETE_SLOT, &heap);
    Ok((Outcome::Normal(Value::Boolean(present)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn noop_setter_impl(_args: Vec<Value>, _this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    Ok((Outcome::Normal(Value::Undefined), heap, fuel))
}

fn number_from_attribute(this: &Value, attr: &str, heap: &Heap) -> f64 {
    element::read_attribute(this, attr, heap)
        .and_then(|text| text.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn number_from_slot(this: &Value, slot: &str, heap: &Heap) -> f64 {
    object_id_from_value(this)
        .and_then(|id| heap.object(id))
        .and_then(|object| match object.get(slot) {
            Some(Value::Number(n)) => Some(*n),
            Some(_) | None => None,
        })
        .unwrap_or(0.0)
}

fn boolean_from_slot(this: &Value, slot: &str, heap: &Heap) -> bool {
    object_id_from_value(this)
        .and_then(|id| heap.object(id))
        .and_then(|object| match object.get(slot) {
            Some(Value::Boolean(b)) => Some(*b),
            Some(_) | None => None,
        })
        .unwrap_or(false)
}

fn stringify_first_arg(args: &[Value]) -> String {
    match args.first() {
        Some(Value::String(s)) => s.clone(),
        Some(other) => format!("{other}"),
        None => String::new(),
    }
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

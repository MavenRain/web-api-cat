//! `Element.innerHTML` (v0.6.5): accessor pair installed on every
//! element.  The getter walks `this.children` and serialises each
//! to HTML; if `this` has no children it falls back to the
//! (HTML-escaped) flattened `textContent`.  The setter parses the
//! assigned string as a fragment (wrapped in
//! `<html><body>...</body></html>` so html-cat's full-document
//! parser handles it via [`crate::document::parse_fragment_children`])
//! and replaces `this.children` in-place with the parsed nodes.
//!
//! The serialiser emits attributes from each element's hidden
//! `__attributes` object in `BTreeMap` order (so the output is
//! deterministic across runs).  Tags and attribute values are
//! escaped per the HTML serialisation algorithm's minimal-rule
//! subset (`<`, `>`, `&`, plus `"` inside attributes).

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{AccessorPair, Object, ObjectId};

use crate::document;

/// Property key under which the `innerHTML` accessor pair lives.
pub const PROPERTY_KEY: &str = "innerHTML";

/// Property key under which the `outerHTML` accessor lives.
pub const OUTER_PROPERTY_KEY: &str = "outerHTML";

/// Install the v0.6.5 `innerHTML` accessor pair on `element_value`.
/// Idempotent: re-running on the same object overwrites the
/// previous accessor.  Called by
/// [`crate::document::build_blank_element`] and
/// `crate::document::build_element` so every element produced by
/// this crate (parsed-HTML and `createElement`'d alike) exposes
/// `innerHTML` for both read and write.
#[must_use]
pub fn install_inner_html_accessor(element_value: &Value, heap: Heap) -> Heap {
    let Some(element_id) = object_id_from_value(element_value) else {
        return heap;
    };
    let Some(element) = heap.object(element_id).cloned() else {
        return heap;
    };
    let accessor = AccessorPair::new(
        Some(Value::Native(inner_html_getter_impl)),
        Some(Value::Native(inner_html_setter_impl)),
    );
    let updated = element.with_accessor(PROPERTY_KEY.to_owned(), accessor);
    heap.store_object(element_id, updated).unwrap_or_else(|h| h)
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn inner_html_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let html = serialize_inner(&this, &heap);
    Ok((Outcome::Normal(Value::String(html)), heap, fuel))
}

/// Install the v0.6.7 `outerHTML` accessor on `element_value`.
/// The getter returns `<tag attrs>inner</tag>` -- the same
/// serialisation `innerHTML` produces for a single element, but
/// including the element's own opening and closing tags.  The
/// setter is currently a no-op: rewriting `outerHTML` requires
/// locating the element's parent and splicing the parsed
/// replacement into the parent's children array, but the JS-side
/// DOM doesn't yet track parent backrefs.  Installing a no-op
/// setter (rather than leaving the accessor read-only) means
/// `el.outerHTML = "..."` runs without throwing, which is what
/// real-world scripts that conditionally rewrite outerHTML expect.
#[must_use]
pub fn install_outer_html_accessor(element_value: &Value, heap: Heap) -> Heap {
    let Some(element_id) = object_id_from_value(element_value) else {
        return heap;
    };
    let Some(element) = heap.object(element_id).cloned() else {
        return heap;
    };
    let accessor = AccessorPair::new(
        Some(Value::Native(outer_html_getter_impl)),
        Some(Value::Native(outer_html_setter_impl)),
    );
    let updated = element.with_accessor(OUTER_PROPERTY_KEY.to_owned(), accessor);
    heap.store_object(element_id, updated).unwrap_or_else(|h| h)
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn outer_html_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let html = object_id_from_value(&this)
        .map(|id| serialize_element(id, &heap))
        .unwrap_or_default();
    Ok((Outcome::Normal(Value::String(html)), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn outer_html_setter_impl(_args: Vec<Value>, _this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    Ok((Outcome::Normal(Value::Undefined), heap, fuel))
}

#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn inner_html_setter_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let input = string_arg(&args, 0);
    let new_heap = replace_children_from_html(&this, &input, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

fn replace_children_from_html(this: &Value, input: &str, heap: Heap) -> Heap {
    let Some(element_id) = object_id_from_value(this) else {
        return heap;
    };
    let Some(element) = heap.object(element_id) else {
        return heap;
    };
    let Some(children_id) = element.get("children").and_then(object_id_from_value) else {
        return heap;
    };
    let (new_children_values, heap) = document::parse_fragment_children(input, heap);
    let new_length = u32::try_from(new_children_values.len()).unwrap_or(u32::MAX);
    let pairs: BTreeMap<String, Value> = new_children_values
        .iter()
        .enumerate()
        .map(|(i, v)| (format!("{i}"), v.clone()))
        .chain(std::iter::once((
            "length".to_owned(),
            Value::Number(f64::from(new_length)),
        )))
        .collect();
    let new_children = Object::from_properties(pairs);
    heap.store_object(children_id, new_children)
        .unwrap_or_else(|h| h)
}

fn serialize_inner(this: &Value, heap: &Heap) -> String {
    let Some(element_id) = object_id_from_value(this) else {
        return String::new();
    };
    let Some(element) = heap.object(element_id) else {
        return String::new();
    };
    let children = children_ids_of(element, heap);
    if children.is_empty() {
        escape_text(&text_content_of(element))
    } else {
        children
            .iter()
            .map(|&id| serialize_element(id, heap))
            .collect()
    }
}

fn serialize_element(element_id: ObjectId, heap: &Heap) -> String {
    let Some(element) = heap.object(element_id) else {
        return String::new();
    };
    let tag = tag_name_of(element);
    let attrs_str = serialize_attributes(element, heap);
    let inner = serialize_inner(&Value::Object(element_id), heap);
    format!("<{tag}{attrs_str}>{inner}</{tag}>")
}

fn serialize_attributes(element: &Object, heap: &Heap) -> String {
    let Some(attrs_id) = attributes_id_of(element) else {
        return String::new();
    };
    let Some(attrs) = heap.object(attrs_id) else {
        return String::new();
    };
    attrs
        .properties()
        .iter()
        .filter_map(|(k, v)| match v {
            Value::String(s) => Some(format!(" {k}=\"{}\"", escape_attribute(s))),
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

fn tag_name_of(element: &Object) -> String {
    element
        .get("tagName")
        .and_then(string_from_value)
        .unwrap_or_default()
}

fn text_content_of(element: &Object) -> String {
    element
        .get("textContent")
        .and_then(string_from_value)
        .unwrap_or_default()
}

fn children_ids_of(element: &Object, heap: &Heap) -> Vec<ObjectId> {
    let Some(children_id) = element.get("children").and_then(object_id_from_value) else {
        return Vec::new();
    };
    let Some(children) = heap.object(children_id) else {
        return Vec::new();
    };
    let length = match children.get("length") {
        Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let len = *n as u32;
            len
        }
        Some(_) | None => 0,
    };
    (0..length)
        .filter_map(|i| children.get(&format!("{i}")).and_then(object_id_from_value))
        .collect()
}

fn attributes_id_of(element: &Object) -> Option<ObjectId> {
    element.get("__attributes").and_then(object_id_from_value)
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

fn string_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::Object(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => None,
    }
}

fn escape_text(s: &str) -> String {
    s.chars().map(escape_text_char).collect()
}

fn escape_attribute(s: &str) -> String {
    s.chars().map(escape_attribute_char).collect()
}

fn escape_text_char(c: char) -> String {
    match c {
        '<' => "&lt;".to_owned(),
        '>' => "&gt;".to_owned(),
        '&' => "&amp;".to_owned(),
        other => other.to_string(),
    }
}

fn escape_attribute_char(c: char) -> String {
    match c {
        '<' => "&lt;".to_owned(),
        '>' => "&gt;".to_owned(),
        '&' => "&amp;".to_owned(),
        '"' => "&quot;".to_owned(),
        other => other.to_string(),
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
            String::new()
        }
    }
}

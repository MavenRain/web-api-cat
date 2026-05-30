//! `Element`-side native methods: `getAttribute`, `setAttribute`,
//! `hasAttribute`, `querySelector`.
//!
//! Element objects are boa-cat [`Object`]s carrying these properties:
//!
//! - `tagName`: lowercased tag name (string).
//! - `id`: id attribute or empty string.
//! - `className`: class attribute or empty string.
//! - `textContent`: concatenated text of all descendant text nodes.
//! - `children`: array of child element objects.
//! - `__attributes`: object mapping attribute name -> value.
//! - `getAttribute`, `setAttribute`, `hasAttribute`,
//!   `querySelector`: native callables.

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{Object, ObjectId};

/// `Element.getAttribute(name)` native implementation.
///
/// # Errors
///
/// Never returns `Err`; bad input yields `Value::Null`.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
pub fn get_attribute_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let name = string_arg(&args, 0);
    let outcome = read_attribute(&this, &name, &heap).map_or(Outcome::Normal(Value::Null), |v| {
        Outcome::Normal(Value::String(v))
    });
    Ok((outcome, heap, fuel))
}

/// `Element.hasAttribute(name)` native implementation.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
pub fn has_attribute_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let name = string_arg(&args, 0);
    let present = read_attribute(&this, &name, &heap).is_some();
    Ok((Outcome::Normal(Value::Boolean(present)), heap, fuel))
}

/// `Element.setAttribute(name, value)` native implementation.
///
/// # Errors
///
/// Never returns `Err`; missing element / attributes object yields the
/// unchanged heap.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
pub fn set_attribute_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let name = string_arg(&args, 0);
    let value = string_arg(&args, 1);
    let new_heap = write_attribute(&this, &name, &value, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
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
        | Value::Native(_) => None,
    }
}

fn attributes_id_of(element: &Object) -> Option<ObjectId> {
    match element.get("__attributes") {
        Some(value) => object_id_of(value),
        None => None,
    }
}

fn read_attribute(this: &Value, name: &str, heap: &Heap) -> Option<String> {
    let element_id = object_id_of(this)?;
    let element = heap.object(element_id)?;
    let attrs_id = attributes_id_of(element)?;
    let attrs = heap.object(attrs_id)?;
    match attrs.get(name) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) | None => None,
    }
}

fn write_attribute(this: &Value, name: &str, value: &str, heap: Heap) -> Heap {
    let Some(element_id) = object_id_of(this) else {
        return heap;
    };
    let Some(element) = heap.object(element_id).cloned() else {
        return heap;
    };
    let Some(attrs_id) = attributes_id_of(&element) else {
        return heap;
    };
    let Some(attrs) = heap.object(attrs_id).cloned() else {
        return heap;
    };
    let updated_attrs = attrs.with(name.to_owned(), Value::String(value.to_owned()));
    let heap = heap
        .store_object(attrs_id, updated_attrs)
        .unwrap_or_else(|h| h);
    let mirror_key = match name {
        "id" => Some("id"),
        "class" => Some("className"),
        _other => None,
    };
    if let Some(key) = mirror_key {
        let updated_element = element.with(key.to_owned(), Value::String(value.to_owned()));
        heap.store_object(element_id, updated_element)
            .unwrap_or_else(|h| h)
    } else {
        heap
    }
}

fn string_arg(args: &[Value], idx: usize) -> String {
    match args.get(idx) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => format!("{other}"),
        None => String::new(),
    }
}

/// Build an attributes object (`__attributes`) from a list of pairs.
#[must_use]
pub fn build_attributes_object(pairs: &[(String, String)], heap: Heap) -> (Value, Heap) {
    let map: BTreeMap<String, Value> = pairs
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    let (id, heap) = heap.alloc_object(Object::from_properties(map));
    (Value::Object(id), heap)
}

/// `Element.querySelector(selector)` -- limited v0 selector subset
/// (`tag`, `.class`, `#id`, `tag.class`, `tag#id`).
///
/// # Errors
///
/// Never returns `Err`; no match yields `Value::Null`.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
pub fn query_selector_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let selector = string_arg(&args, 0);
    let pattern = parse_simple_selector(&selector);
    let outcome =
        find_matching(&this, &pattern, &heap).map_or(Outcome::Normal(Value::Null), Outcome::Normal);
    Ok((outcome, heap, fuel))
}

#[derive(Debug, Clone, Default)]
struct SelectorPattern {
    tag: Option<String>,
    id: Option<String>,
    class: Option<String>,
}

fn parse_simple_selector(source: &str) -> SelectorPattern {
    let trimmed = source.trim();
    parse_selector_recursive(trimmed, 0, SelectorPattern::default())
}

fn parse_selector_recursive(source: &str, start: usize, acc: SelectorPattern) -> SelectorPattern {
    let bytes = source.as_bytes();
    if start >= bytes.len() {
        acc
    } else {
        match bytes.get(start) {
            Some(b'#') => {
                let end = scan_ident(bytes, start + 1);
                let name = source.get(start + 1..end).unwrap_or("").to_owned();
                parse_selector_recursive(
                    source,
                    end,
                    SelectorPattern {
                        id: Some(name),
                        ..acc
                    },
                )
            }
            Some(b'.') => {
                let end = scan_ident(bytes, start + 1);
                let name = source.get(start + 1..end).unwrap_or("").to_owned();
                parse_selector_recursive(
                    source,
                    end,
                    SelectorPattern {
                        class: Some(name),
                        ..acc
                    },
                )
            }
            Some(_) => {
                let end = scan_ident(bytes, start);
                let name = source.get(start..end).unwrap_or("").to_ascii_lowercase();
                if name.is_empty() {
                    acc
                } else {
                    parse_selector_recursive(
                        source,
                        end,
                        SelectorPattern {
                            tag: Some(name),
                            ..acc
                        },
                    )
                }
            }
            None => acc,
        }
    }
}

fn scan_ident(bytes: &[u8], start: usize) -> usize {
    bytes
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, b)| !is_ident_byte(**b))
        .map_or(bytes.len(), |(i, _)| i)
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn find_matching(this: &Value, pattern: &SelectorPattern, heap: &Heap) -> Option<Value> {
    let root_id = object_id_of(this)?;
    walk_descendants(root_id, pattern, heap)
}

fn walk_descendants(node_id: ObjectId, pattern: &SelectorPattern, heap: &Heap) -> Option<Value> {
    let children = element_children(node_id, heap);
    children.iter().find_map(|child_id| {
        if matches_pattern(*child_id, pattern, heap) {
            Some(Value::Object(*child_id))
        } else {
            walk_descendants(*child_id, pattern, heap)
        }
    })
}

fn element_children(node_id: ObjectId, heap: &Heap) -> Vec<ObjectId> {
    let Some(object) = heap.object(node_id) else {
        return Vec::new();
    };
    let Some(children_id) = object.get("children").and_then(object_id_of) else {
        return Vec::new();
    };
    let Some(children) = heap.object(children_id) else {
        return Vec::new();
    };
    let length = array_length(children);
    (0..length)
        .filter_map(|i| children.get(&format!("{i}")).and_then(object_id_of))
        .collect()
}

fn array_length(array: &Object) -> u32 {
    match array.get("length") {
        Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let len = *n as u32;
            len
        }
        Some(_) | None => 0,
    }
}

fn matches_pattern(node_id: ObjectId, pattern: &SelectorPattern, heap: &Heap) -> bool {
    let Some(object) = heap.object(node_id) else {
        return false;
    };
    let tag_ok = pattern
        .tag
        .as_ref()
        .is_none_or(|want| string_property(object, "tagName").eq_ignore_ascii_case(want));
    let id_ok = pattern
        .id
        .as_ref()
        .is_none_or(|want| string_property(object, "id") == *want);
    let class_ok = pattern.class.as_ref().is_none_or(|want| {
        string_property(object, "className")
            .split_ascii_whitespace()
            .any(|c| c == want)
    });
    tag_ok && id_ok && class_ok
}

fn string_property(object: &Object, key: &str) -> String {
    match object.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(_) | None => String::new(),
    }
}

/// Public helper used by `document.querySelector` to perform the search
/// from the document root.
#[must_use]
pub fn find_first_descendant(root: ObjectId, pattern_source: &str, heap: &Heap) -> Option<Value> {
    let pattern = parse_simple_selector(pattern_source);
    walk_descendants(root, &pattern, heap)
}

/// Public helper for `document.getElementById`.
#[must_use]
pub fn find_by_id(root: ObjectId, id: &str, heap: &Heap) -> Option<Value> {
    let pattern = SelectorPattern {
        id: Some(id.to_owned()),
        ..SelectorPattern::default()
    };
    walk_descendants(root, &pattern, heap)
}

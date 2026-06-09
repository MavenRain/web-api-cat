//! Translate an `html-cat` document into a tree of boa-cat element
//! objects.

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{Object, ObjectId};
use html_cat::{Document as HtmlDoc, Element as HtmlElement, Node as HtmlNode};

use crate::element;

/// Build the JS-side document object tree from `html_doc`.  Returns
/// `(document_value, root_element_value, heap)`.
#[must_use]
pub fn build(html_doc: &HtmlDoc, heap: Heap) -> (Value, Value, Heap) {
    let (root_value, heap) = build_element(html_doc.root(), heap);
    build_document_object(root_value, heap)
}

fn build_document_object(root_value: Value, heap: Heap) -> (Value, Value, Heap) {
    let body_value = object_id_of(&root_value)
        .and_then(|id| find_element_by_tag(id, "body", &heap))
        .unwrap_or(Value::Null);
    let mut props = BTreeMap::new();
    let _ = props.insert("documentElement".to_owned(), root_value.clone());
    let _ = props.insert("body".to_owned(), body_value);
    let _ = props.insert(
        "getElementById".to_owned(),
        Value::Native(get_element_by_id_impl),
    );
    let _ = props.insert(
        "querySelector".to_owned(),
        Value::Native(document_query_selector_impl),
    );
    let (doc_id, heap) = heap.alloc_object(Object::from_properties(props));
    let doc_value = Value::Object(doc_id);
    let heap = crate::cookie::install_cookie_accessor(&doc_value, heap);
    (doc_value, root_value, heap)
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

fn find_element_by_tag(node_id: ObjectId, tag: &str, heap: &Heap) -> Option<Value> {
    let object = heap.object(node_id)?;
    let tag_name = match object.get("tagName") {
        Some(Value::String(s)) => s.as_str(),
        Some(_) | None => "",
    };
    if tag_name.eq_ignore_ascii_case(tag) {
        Some(Value::Object(node_id))
    } else {
        let children_id = object.get("children").and_then(object_id_of)?;
        let children_obj = heap.object(children_id)?;
        let length = match children_obj.get("length") {
            Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let len = *n as u32;
                len
            }
            Some(_) | None => 0,
        };
        (0..length).find_map(|i| {
            children_obj
                .get(&format!("{i}"))
                .and_then(object_id_of)
                .and_then(|child_id| find_element_by_tag(child_id, tag, heap))
        })
    }
}

fn build_element(html_element: &HtmlElement, heap: Heap) -> (Value, Heap) {
    let attribute_pairs: Vec<(String, String)> = html_element
        .attributes()
        .iter()
        .map(|a| (a.name().to_owned(), a.value().to_owned()))
        .collect();
    let id = lookup_attribute(&attribute_pairs, "id").unwrap_or_default();
    let class = lookup_attribute(&attribute_pairs, "class").unwrap_or_default();
    let text_content = collect_text(html_element.children());
    let (attributes_value, heap) = element::build_attributes_object(&attribute_pairs, heap);
    let (children_values, heap) = build_children(html_element.children(), heap);
    let (children_array_value, heap) = build_array_object(&children_values, heap);
    let mut props = BTreeMap::new();
    let _ = props.insert(
        "tagName".to_owned(),
        Value::String(html_element.name().to_owned()),
    );
    let _ = props.insert("id".to_owned(), Value::String(id));
    let _ = props.insert("className".to_owned(), Value::String(class));
    let _ = props.insert("textContent".to_owned(), Value::String(text_content));
    let _ = props.insert("children".to_owned(), children_array_value);
    let _ = props.insert("__attributes".to_owned(), attributes_value);
    let _ = props.insert(
        "getAttribute".to_owned(),
        Value::Native(element::get_attribute_impl),
    );
    let _ = props.insert(
        "setAttribute".to_owned(),
        Value::Native(element::set_attribute_impl),
    );
    let _ = props.insert(
        "hasAttribute".to_owned(),
        Value::Native(element::has_attribute_impl),
    );
    let _ = props.insert(
        "querySelector".to_owned(),
        Value::Native(element::query_selector_impl),
    );
    let (id, heap) = heap.alloc_object(Object::from_properties(props));
    (Value::Object(id), heap)
}

fn build_children(children: &[HtmlNode], heap: Heap) -> (Vec<Value>, Heap) {
    children
        .iter()
        .fold((Vec::new(), heap), |(acc, heap), child| match child {
            HtmlNode::Element(e) => {
                let (value, heap) = build_element(e, heap);
                let extended: Vec<Value> = acc.into_iter().chain(std::iter::once(value)).collect();
                (extended, heap)
            }
            HtmlNode::Text(_) | HtmlNode::Comment(_) => (acc, heap),
        })
}

fn build_array_object(values: &[Value], heap: Heap) -> (Value, Heap) {
    let length = u32::try_from(values.len()).unwrap_or(u32::MAX);
    let map: BTreeMap<String, Value> = values
        .iter()
        .enumerate()
        .map(|(i, v)| (format!("{i}"), v.clone()))
        .chain(std::iter::once((
            "length".to_owned(),
            Value::Number(f64::from(length)),
        )))
        .collect();
    let (id, heap) = heap.alloc_object(Object::from_properties(map));
    (Value::Object(id), heap)
}

fn lookup_attribute(pairs: &[(String, String)], name: &str) -> Option<String> {
    pairs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
}

fn collect_text(children: &[HtmlNode]) -> String {
    children
        .iter()
        .fold(String::new(), |acc, child| match child {
            HtmlNode::Text(t) => format!("{acc}{}", t.content()),
            HtmlNode::Element(e) => format!("{acc}{}", collect_text(e.children())),
            HtmlNode::Comment(_) => acc,
        })
}

fn document_root_id(this: &Value, heap: &Heap) -> Option<ObjectId> {
    let document_id = object_id_of(this)?;
    let document = heap.object(document_id)?;
    document.get("documentElement").and_then(object_id_of)
}

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
fn get_element_by_id_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let id_arg = match args.first() {
        Some(Value::String(s)) => s.clone(),
        Some(_) | None => String::new(),
    };
    let outcome = document_root_id(&this, &heap)
        .and_then(|root_id| element::find_by_id(root_id, &id_arg, &heap))
        .map_or(Outcome::Normal(Value::Null), Outcome::Normal);
    Ok((outcome, heap, fuel))
}

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
fn document_query_selector_impl(
    args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let selector = match args.first() {
        Some(Value::String(s)) => s.clone(),
        Some(_) | None => String::new(),
    };
    let outcome = document_root_id(&this, &heap)
        .and_then(|root_id| element::find_first_descendant(root_id, &selector, &heap))
        .map_or(Outcome::Normal(Value::Null), Outcome::Normal);
    Ok((outcome, heap, fuel))
}

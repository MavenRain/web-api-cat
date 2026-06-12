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
    let _ = props.insert(
        "createElement".to_owned(),
        Value::Native(create_element_impl),
    );
    let (doc_id, heap) = heap.alloc_object(Object::from_properties(props));
    let doc_value = Value::Object(doc_id);
    let heap = crate::cookie::install_cookie_accessor(&doc_value, heap);
    (doc_value, root_value, heap)
}

/// `document.createElement(tagName)` (v0.6.1): allocate a fresh
/// element-shaped object on the heap.  The new element has the
/// requested `tagName`, empty `id` / `className` / `textContent`,
/// an empty children array, an empty `__attributes` object, and
/// every method binding a build-time element carries (so the
/// caller can chain `setAttribute`, `appendChild`, etc. straight
/// away).
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn create_element_impl(args: Vec<Value>, _this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let tag = match args.first() {
        Some(Value::String(s)) => s.clone(),
        Some(
            Value::Undefined
            | Value::Null
            | Value::Boolean(_)
            | Value::Number(_)
            | Value::Object(_)
            | Value::Function(_)
            | Value::Native(_)
            | Value::Promise(_),
        )
        | None => String::new(),
    };
    let (element, heap) = build_blank_element(&tag, heap);
    Ok((Outcome::Normal(element), heap, fuel))
}

/// v0.6.1 helper: build an element with the supplied tagName but
/// otherwise-empty fields.  Used by both `createElement` and the
/// `appendChild` test setup; the rendered fields match what
/// [`build_element`] produces for an HTML-parsed element so
/// [`crate::extract::extract_document`] picks the new node up on
/// the next back-prop pass.
#[must_use]
pub fn build_blank_element(tag: &str, heap: Heap) -> (Value, Heap) {
    let attribute_pairs: Vec<(String, String)> = Vec::new();
    let (attributes_value, heap) = element::build_attributes_object(&attribute_pairs, heap);
    let (children_array_value, heap) = build_array_object(&[], heap);
    let mut props = BTreeMap::new();
    let _ = props.insert("tagName".to_owned(), Value::String(tag.to_owned()));
    let _ = props.insert("id".to_owned(), Value::String(String::new()));
    let _ = props.insert("className".to_owned(), Value::String(String::new()));
    let _ = props.insert("textContent".to_owned(), Value::String(String::new()));
    let _ = props.insert("children".to_owned(), children_array_value);
    let _ = props.insert("__attributes".to_owned(), attributes_value);
    let _ = props.insert("__parent__".to_owned(), Value::Null);
    let (style_value, heap) = build_empty_style_object(heap);
    let _ = props.insert("style".to_owned(), style_value);
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
    let _ = props.insert(
        "appendChild".to_owned(),
        Value::Native(element::append_child_impl),
    );
    let _ = props.insert(
        "removeChild".to_owned(),
        Value::Native(element::remove_child_impl),
    );
    let _ = props.insert(
        "insertBefore".to_owned(),
        Value::Native(element::insert_before_impl),
    );
    let _ = props.insert(
        "replaceChild".to_owned(),
        Value::Native(element::replace_child_impl),
    );
    let _ = props.insert("cloneNode".to_owned(), Value::Native(clone_node_impl));
    let _ = props.insert("remove".to_owned(), Value::Native(element::remove_impl));
    let (id, heap) = heap.alloc_object(Object::from_properties(props));
    let heap = install_class_list(id, heap);
    let heap = install_parent_accessors(id, heap);
    let heap = crate::inner_html::install_inner_html_accessor(&Value::Object(id), heap);
    let heap = crate::inner_html::install_outer_html_accessor(&Value::Object(id), heap);
    (Value::Object(id), heap)
}

/// v0.6.8 helper: install `parentElement` and `parentNode` as
/// accessors with getters that read `this.__parent__`.  Same
/// underlying `NativeFn` (`element::parent_getter_impl`) -- we
/// don't yet distinguish elements from other node types, so both
/// accessors produce identical results.  Setter is omitted; the
/// engine reports the property as read-only.
fn install_parent_accessors(element_id: ObjectId, heap: Heap) -> Heap {
    let Some(element) = heap.object(element_id).cloned() else {
        return heap;
    };
    let parent_element_pair =
        boa_cat::value::AccessorPair::new(Some(Value::Native(element::parent_getter_impl)), None);
    let parent_node_pair =
        boa_cat::value::AccessorPair::new(Some(Value::Native(element::parent_getter_impl)), None);
    let updated = element
        .with_accessor("parentElement".to_owned(), parent_element_pair)
        .with_accessor("parentNode".to_owned(), parent_node_pair);
    heap.store_object(element_id, updated).unwrap_or_else(|h| h)
}

/// v0.6.4 helper: allocate a fresh empty Object to attach as
/// `element.style`.  Scripts read and write arbitrary keys
/// (`el.style.color = 'red'`) and the engine's normal property
/// storage handles the round-trip; no special methods needed.  The
/// inline `style="..."` attribute on parsed HTML is NOT parsed
/// into this object (it stays readable via `getAttribute('style')`);
/// real-world `CSSStyleDeclaration` parsing is deferred.  Back-prop
/// from this object into layout-cat is also deferred -- this v0
/// chunk targets script-compatibility, not visual styling.
fn build_empty_style_object(heap: Heap) -> (Value, Heap) {
    let (id, heap) = heap.alloc_object(Object::from_properties(BTreeMap::new()));
    (Value::Object(id), heap)
}

/// v0.6.3 helper: allocate a `classList` Object holding an
/// `__element__` backref to `element_id` and the four
/// `DOMTokenList` natives (`add`, `remove`, `contains`, `toggle`),
/// then attach it as `classList` on the element via
/// `store_object`.  Run after the element itself is allocated so
/// the backref can be materialised.
fn install_class_list(element_id: ObjectId, heap: Heap) -> Heap {
    let (class_list_value, heap) = build_class_list_object(element_id, heap);
    let Some(element) = heap.object(element_id).cloned() else {
        return heap;
    };
    let updated = element.with("classList".to_owned(), class_list_value);
    heap.store_object(element_id, updated).unwrap_or_else(|h| h)
}

fn build_class_list_object(element_id: ObjectId, heap: Heap) -> (Value, Heap) {
    let mut props = BTreeMap::new();
    let _ = props.insert("__element__".to_owned(), Value::Object(element_id));
    let _ = props.insert(
        "add".to_owned(),
        Value::Native(element::class_list_add_impl),
    );
    let _ = props.insert(
        "remove".to_owned(),
        Value::Native(element::class_list_remove_impl),
    );
    let _ = props.insert(
        "contains".to_owned(),
        Value::Native(element::class_list_contains_impl),
    );
    let _ = props.insert(
        "toggle".to_owned(),
        Value::Native(element::class_list_toggle_impl),
    );
    let (id, heap) = heap.alloc_object(Object::from_properties(props));
    (Value::Object(id), heap)
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
    let _ = props.insert("__parent__".to_owned(), Value::Null);
    let (style_value, heap) = build_empty_style_object(heap);
    let _ = props.insert("style".to_owned(), style_value);
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
    let _ = props.insert(
        "appendChild".to_owned(),
        Value::Native(element::append_child_impl),
    );
    let _ = props.insert(
        "removeChild".to_owned(),
        Value::Native(element::remove_child_impl),
    );
    let _ = props.insert(
        "insertBefore".to_owned(),
        Value::Native(element::insert_before_impl),
    );
    let _ = props.insert(
        "replaceChild".to_owned(),
        Value::Native(element::replace_child_impl),
    );
    let _ = props.insert("cloneNode".to_owned(), Value::Native(clone_node_impl));
    let _ = props.insert("remove".to_owned(), Value::Native(element::remove_impl));
    let (id, heap) = heap.alloc_object(Object::from_properties(props));
    let heap = children_values.iter().fold(heap, |heap, child| {
        element::set_parent_backref(child, id, heap)
    });
    let heap = install_class_list(id, heap);
    let heap = install_parent_accessors(id, heap);
    let heap = crate::inner_html::install_inner_html_accessor(&Value::Object(id), heap);
    let heap = crate::inner_html::install_outer_html_accessor(&Value::Object(id), heap);
    (Value::Object(id), heap)
}

/// `Element.cloneNode(deep)` (v0.6.6): produce an independent copy
/// of `this`.  `deep === true` clones every descendant
/// recursively; the default (no argument or anything that isn't
/// the literal `true`) clones only the element itself with an
/// empty children array.  Returns `null` if `this` isn't an
/// Object.
///
/// The clone has fresh `ObjectId`s for the element, its
/// `__attributes`, its `style`, its `classList` (with a backref to
/// the new element id), and its children array.  Native-method
/// bindings (`getAttribute`, `appendChild`, ...) are copied by
/// value (function pointers, no per-instance state).  The
/// `innerHTML` accessor is freshly installed on the clone.  All of
/// that means mutating `clone.setAttribute(...)`,
/// `clone.classList.add(...)`, `clone.style.x = ...`, or
/// `clone.appendChild(...)` cannot bleed back into the original.
///
/// # Errors
///
/// Never returns `Err`; bad inputs yield `null`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn clone_node_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let deep = matches!(args.first(), Some(Value::Boolean(true)));
    let (clone_value, heap) = if let Some(id) = object_id_of(&this) {
        clone_element(id, deep, heap)
    } else {
        (Value::Null, heap)
    };
    Ok((Outcome::Normal(clone_value), heap, fuel))
}

fn clone_element(element_id: ObjectId, deep: bool, heap: Heap) -> (Value, Heap) {
    let Some(original) = heap.object(element_id).cloned() else {
        return (Value::Null, heap);
    };
    let attrs_id_opt = original.get("__attributes").and_then(object_id_of);
    let (cloned_attrs_id_opt, heap) = clone_object_option(attrs_id_opt, heap);
    let style_id_opt = original.get("style").and_then(object_id_of);
    let (cloned_style_id_opt, heap) = clone_object_option(style_id_opt, heap);
    let children_id_opt = original.get("children").and_then(object_id_of);
    let (new_children_values, heap) = if deep {
        if let Some(cid) = children_id_opt {
            clone_children_deep(cid, heap)
        } else {
            (Vec::new(), heap)
        }
    } else {
        (Vec::new(), heap)
    };
    let (new_children_array_value, heap) = build_array_object(&new_children_values, heap);
    let new_props: BTreeMap<String, Value> = original
        .properties()
        .iter()
        .filter(|(k, _)| {
            !matches!(
                k.as_str(),
                "__attributes" | "style" | "children" | "classList" | "__parent__"
            )
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .chain(cloned_attrs_id_opt.map(|id| ("__attributes".to_owned(), Value::Object(id))))
        .chain(cloned_style_id_opt.map(|id| ("style".to_owned(), Value::Object(id))))
        .chain(std::iter::once((
            "children".to_owned(),
            new_children_array_value,
        )))
        .chain(std::iter::once(("__parent__".to_owned(), Value::Null)))
        .collect();
    let (new_element_id, heap) = heap.alloc_object(Object::from_properties(new_props));
    let heap = new_children_values.iter().fold(heap, |heap, child| {
        element::set_parent_backref(child, new_element_id, heap)
    });
    let heap = install_class_list(new_element_id, heap);
    let heap = install_parent_accessors(new_element_id, heap);
    let heap = crate::inner_html::install_inner_html_accessor(&Value::Object(new_element_id), heap);
    let heap = crate::inner_html::install_outer_html_accessor(&Value::Object(new_element_id), heap);
    (Value::Object(new_element_id), heap)
}

fn clone_object_option(id_opt: Option<ObjectId>, heap: Heap) -> (Option<ObjectId>, Heap) {
    let Some(id) = id_opt else {
        return (None, heap);
    };
    let Some(obj) = heap.object(id).cloned() else {
        return (None, heap);
    };
    let (new_id, heap) = heap.alloc_object(obj);
    (Some(new_id), heap)
}

fn clone_children_deep(children_id: ObjectId, heap: Heap) -> (Vec<Value>, Heap) {
    let Some(children) = heap.object(children_id).cloned() else {
        return (Vec::new(), heap);
    };
    let length = match children.get("length") {
        Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let len = *n as u32;
            len
        }
        Some(_) | None => 0,
    };
    (0..length).fold((Vec::new(), heap), |(acc, heap), i| {
        if let Some(child_id) = children.get(&format!("{i}")).and_then(object_id_of) {
            let (cloned, h) = clone_element(child_id, true, heap);
            let acc = acc.into_iter().chain(std::iter::once(cloned)).collect();
            (acc, h)
        } else {
            (acc, heap)
        }
    })
}

/// v0.6.5 helper used by [`crate::inner_html`]: parse `html` as a
/// fragment (wrapped in `<html><body>...</body></html>` so the
/// html-cat full-document parser handles it), locate the body
/// element, and run [`build_children`] over its child nodes.
/// Returns the produced child element values plus the updated
/// heap.  On parse failure returns an empty vector and the
/// unchanged heap so the caller can treat it as a no-op.
#[must_use]
pub fn parse_fragment_children(html: &str, heap: Heap) -> (Vec<Value>, Heap) {
    let wrapped = format!("<html><body>{html}</body></html>");
    let Ok(doc) = html_cat::parse(&wrapped) else {
        return (Vec::new(), heap);
    };
    let root = doc.root();
    let body = find_html_body(root).unwrap_or(root);
    build_children(body.children(), heap)
}

fn find_html_body(element: &HtmlElement) -> Option<&HtmlElement> {
    if element.name().eq_ignore_ascii_case("body") {
        Some(element)
    } else {
        element.children().iter().find_map(|c| match c {
            HtmlNode::Element(e) => find_html_body(e),
            HtmlNode::Text(_) | HtmlNode::Comment(_) => None,
        })
    }
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

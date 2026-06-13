//! Install the web APIs (`document`, `fetch`, `window`,
//! `localStorage`, `sessionStorage`) into a boa-cat environment
//! and heap.

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::env::Env;
use boa_cat::heap::Heap;
use boa_cat::value::{Cell, Object};
use html_cat::Document as HtmlDoc;

use crate::document::build as build_document;
use crate::fetch::fetch_impl;
use crate::storage::build_storage_object;

/// Fold the web APIs and the parsed document into `env` and `heap`,
/// returning the extended pair.  Each binding (document, fetch,
/// window, localStorage, sessionStorage) lands as a const cell so
/// scripts can't rebind it.  v0.7.0 adds the two storages plus a
/// proper `window` Object whose `document` / `localStorage` /
/// `sessionStorage` properties point at the same Values bound at
/// the top level.
#[must_use]
pub fn install(env: Env, heap: Heap, html_doc: &HtmlDoc) -> (Env, Heap) {
    let (document_value, _root_value, heap) = build_document(html_doc, heap);
    let (local_storage_value, heap) = build_storage_object(heap);
    let (session_storage_value, heap) = build_storage_object(heap);
    let (window_value, heap) = build_window_object(
        &document_value,
        &local_storage_value,
        &session_storage_value,
        heap,
    );
    let bindings: Vec<(&str, Value)> = vec![
        ("document", document_value),
        ("fetch", Value::Native(fetch_impl)),
        ("window", window_value),
        ("localStorage", local_storage_value),
        ("sessionStorage", session_storage_value),
    ];
    bindings
        .into_iter()
        .fold((env, heap), |(env, heap), (name, value)| {
            let (cell_id, heap) = heap.alloc_cell(Cell::new(value, false));
            (env.extend_cell(name, cell_id), heap)
        })
}

fn build_window_object(
    document_value: &Value,
    local_storage_value: &Value,
    session_storage_value: &Value,
    heap: Heap,
) -> (Value, Heap) {
    let mut props = BTreeMap::new();
    let _ = props.insert("document".to_owned(), document_value.clone());
    let _ = props.insert("localStorage".to_owned(), local_storage_value.clone());
    let _ = props.insert("sessionStorage".to_owned(), session_storage_value.clone());
    let (id, heap) = heap.alloc_object(Object::from_properties(props));
    (Value::Object(id), heap)
}

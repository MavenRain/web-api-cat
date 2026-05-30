//! Install the web APIs (`document`, `fetch`, `window`) into a boa-cat
//! environment and heap.

use boa_cat::Value;
use boa_cat::env::Env;
use boa_cat::heap::Heap;
use boa_cat::value::Cell;
use html_cat::Document as HtmlDoc;

use crate::document::build as build_document;
use crate::fetch::fetch_impl;

/// Fold the web APIs and the parsed document into `env` and `heap`,
/// returning the extended pair.  Each binding (document, fetch,
/// window) lands as a const cell so scripts can't rebind it.
#[must_use]
pub fn install(env: Env, heap: Heap, html_doc: &HtmlDoc) -> (Env, Heap) {
    let (document_value, _root_value, heap) = build_document(html_doc, heap);
    let bindings: Vec<(&str, Value)> = vec![
        ("document", document_value.clone()),
        ("fetch", Value::Native(fetch_impl)),
        ("window", document_value),
    ];
    bindings
        .into_iter()
        .fold((env, heap), |(env, heap), (name, value)| {
            let (cell_id, heap) = heap.alloc_cell(Cell::new(value, false));
            (env.extend_cell(name, cell_id), heap)
        })
}

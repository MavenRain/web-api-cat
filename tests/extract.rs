//! Integration tests for `extract_document` (back-prop into dom-cat).

use boa_cat::env::Env;
use boa_cat::evaluate_program_with;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::value::ObjectId;
use boa_cat::{Value, value::Cell};
use dom_cat::{Arena, ElementData, Node, NodeId};
use ecma_lex_cat::lex;
use ecma_parse_cat::parse_script;
use web_api_cat::document::build as build_document;
use web_api_cat::{Error, extract_document};

fn fail(_msg: &'static str) -> Error {
    Error::Engine(boa_cat::Error::Unsupported { feature: "test" })
}

fn install_with_document_handle(html: &str) -> Result<(Env, Heap, Value), Error> {
    let html_doc = html_cat::parse(html)?;
    let (document_value, _root, heap) = build_document(&html_doc, Heap::new());
    let (env, heap) = web_api_cat::install(Env::empty(), heap, &html_doc);
    // `install` builds its own document; we want the same one we hold a
    // handle to.  Re-extend env with our pre-built document under the
    // same name.
    let (cell_id, heap) = heap.alloc_cell(Cell::new(document_value.clone(), false));
    let env = env.extend_cell("document", cell_id);
    Ok((env, heap, document_value))
}

fn run_and_extract(html: &str, script: &str) -> Result<dom_cat::Document, Error> {
    let (env, heap, document_value) = install_with_document_handle(html)?;
    let tokens = lex(script).map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (_value, heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(100_000)).map_err(Error::from)?;
    extract_document(&document_value, &heap).ok_or_else(|| fail("extract_document returned None"))
}

fn find_element<'a>(
    arena: &'a Arena,
    root: NodeId,
    tag: &str,
) -> Option<(&'a ElementData, NodeId)> {
    let node = arena.get(root)?;
    let element_match = node
        .as_element()
        .filter(|element| element.name().eq_ignore_ascii_case(tag))
        .map(|element| (element, root));
    element_match.or_else(|| {
        node.children()
            .iter()
            .find_map(|child| find_element(arena, *child, tag))
    })
}

#[test]
fn extract_returns_document_node_at_root() -> Result<(), Error> {
    let document = run_and_extract("<html><body><p>hi</p></body></html>", "1")?;
    let root = document
        .arena()
        .get(document.root())
        .ok_or_else(|| fail("missing root node"))?;
    matches!(root, Node::Document(_))
        .then_some(())
        .ok_or_else(|| fail("expected Document node at root"))
}

#[test]
fn extract_preserves_element_tree() -> Result<(), Error> {
    let document = run_and_extract("<html><body><p id='greet'>hello</p></body></html>", "1")?;
    let (_p, p_id) =
        find_element(document.arena(), document.root(), "p").ok_or_else(|| fail("no p element"))?;
    let p_node = document
        .arena()
        .get(p_id)
        .ok_or_else(|| fail("p missing"))?;
    let has_text_child = p_node.children().iter().any(|child| {
        document
            .arena()
            .get(*child)
            .is_some_and(|n| matches!(n, Node::Text(t) if t.content() == "hello"))
    });
    has_text_child
        .then_some(())
        .ok_or_else(|| fail("expected p to have a 'hello' text child"))
}

#[test]
fn extract_reflects_set_attribute_mutation() -> Result<(), Error> {
    let document = run_and_extract(
        "<html><body><p id='p'>x</p></body></html>",
        "document.getElementById('p').setAttribute('data-x', '42')",
    )?;
    let (p, _id) =
        find_element(document.arena(), document.root(), "p").ok_or_else(|| fail("no p element"))?;
    p.attribute("data-x")
        .filter(|value| *value == "42")
        .map(|_value| ())
        .ok_or_else(|| fail("expected data-x='42' after setAttribute"))
}

#[test]
fn extract_returns_none_for_non_object_value() -> Result<(), Error> {
    let heap = Heap::new();
    let result = extract_document(&Value::Null, &heap);
    result
        .is_none()
        .then_some(())
        .ok_or_else(|| fail("expected None for non-object input"))
}

#[test]
fn extract_returns_none_when_document_missing_root() -> Result<(), Error> {
    use boa_cat::value::Object;
    use std::collections::BTreeMap;
    let heap = Heap::new();
    let (doc_id, heap) = heap.alloc_object(Object::from_properties(BTreeMap::new()));
    let document_value = Value::Object(doc_id);
    let result = extract_document(&document_value, &heap);
    result
        .is_none()
        .then_some(())
        .ok_or_else(|| fail("expected None when documentElement is missing"))
}

#[allow(dead_code)]
fn _unused_object_id(_id: ObjectId) {}

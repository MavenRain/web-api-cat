//! Inline `style="..."` parsing + back-prop (v0.7.2).

use boa_cat::Value;
use boa_cat::env::Env;
use boa_cat::evaluate_program_with;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::value::Cell;
use dom_cat::{Arena, ElementData, NodeId};
use ecma_lex_cat::lex;
use ecma_parse_cat::parse_script;
use web_api_cat::document::build as build_document;
use web_api_cat::{Error, extract_document};

fn fail(_msg: &'static str) -> Error {
    Error::Engine(boa_cat::Error::Unsupported { feature: "test" })
}

fn run_script(html: &str, script: &str) -> Result<Value, Error> {
    let html_doc = html_cat::parse(html)?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let tokens = lex(script).map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, _heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(200_000)).map_err(Error::from)?;
    Ok(value)
}

fn install_with_document_handle(html: &str) -> Result<(Env, Heap, Value), Error> {
    let html_doc = html_cat::parse(html)?;
    let (document_value, _root, heap) = build_document(&html_doc, Heap::new());
    let (env, heap) = web_api_cat::install(Env::empty(), heap, &html_doc);
    let (cell_id, heap) = heap.alloc_cell(Cell::new(document_value.clone(), false));
    let env = env.extend_cell("document", cell_id);
    Ok((env, heap, document_value))
}

fn run_and_extract(html: &str, script: &str) -> Result<dom_cat::Document, Error> {
    let (env, heap, document_value) = install_with_document_handle(html)?;
    let tokens = lex(script).map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (_value, heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(200_000)).map_err(Error::from)?;
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

fn style_attribute(element: &ElementData) -> Option<&str> {
    element
        .attributes()
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("style"))
        .map(|(_, value)| value.as_str())
}

#[test]
fn parsed_style_attribute_populates_style_object_camelcase() -> Result<(), Error> {
    let value = run_script(
        "<html><body><div id='host' style='color: red'></div></body></html>",
        "document.getElementById('host').style.color",
    )?;
    matches!(value, Value::String(ref s) if s == "red")
        .then_some(())
        .ok_or_else(|| fail("expected style.color to be 'red' from parsed style attribute"))
}

#[test]
fn parsed_style_kebab_attribute_becomes_camelcase_property() -> Result<(), Error> {
    let value = run_script(
        "<html><body><div id='host' style='font-size: 14px'></div></body></html>",
        "document.getElementById('host').style.fontSize",
    )?;
    matches!(value, Value::String(ref s) if s == "14px")
        .then_some(())
        .ok_or_else(|| fail("expected font-size to be readable as style.fontSize"))
}

#[test]
fn parsed_style_multiple_declarations() -> Result<(), Error> {
    let value = run_script(
        "<html><body><div id='host' style='color: red; padding: 5px'></div></body></html>",
        "const host = document.getElementById('host');
        host.style.color + ',' + host.style.padding",
    )?;
    matches!(value, Value::String(ref s) if s == "red,5px")
        .then_some(())
        .ok_or_else(|| fail("expected both style props to be readable"))
}

#[test]
fn parsed_style_tolerates_whitespace_and_trailing_semicolon() -> Result<(), Error> {
    let value = run_script(
        "<html><body><div id='host' style='  color :  red ; '></div></body></html>",
        "document.getElementById('host').style.color",
    )?;
    matches!(value, Value::String(ref s) if s == "red")
        .then_some(())
        .ok_or_else(|| fail("expected whitespace/trailing-semicolon to be tolerated"))
}

#[test]
fn parsed_style_compound_property_round_trip() -> Result<(), Error> {
    let value = run_script(
        "<html><body><div id='host' style='border-bottom-color: blue'></div></body></html>",
        "document.getElementById('host').style.borderBottomColor",
    )?;
    matches!(value, Value::String(ref s) if s == "blue")
        .then_some(())
        .ok_or_else(|| fail("expected three-segment kebab to camelCase correctly"))
}

#[test]
fn extract_serializes_style_object_back_to_kebab() -> Result<(), Error> {
    let doc = run_and_extract(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.style.color = 'red';
        host.style.fontSize = '14px';",
    )?;
    let (element, _) =
        find_element(doc.arena(), doc.root(), "div").ok_or_else(|| fail("div not found"))?;
    let style = style_attribute(element).ok_or_else(|| fail("style attribute missing"))?;
    style
        .contains("color: red")
        .then_some(())
        .ok_or_else(|| fail("expected 'color: red' in serialised style"))?;
    style
        .contains("font-size: 14px")
        .then_some(())
        .ok_or_else(|| fail("expected 'font-size: 14px' in serialised style"))
}

#[test]
fn extract_drops_style_attribute_when_style_object_is_empty() -> Result<(), Error> {
    let doc = run_and_extract(
        "<html><body><div id='host'></div></body></html>",
        "/* no style writes */",
    )?;
    let (element, _) =
        find_element(doc.arena(), doc.root(), "div").ok_or_else(|| fail("div not found"))?;
    style_attribute(element)
        .is_none()
        .then_some(())
        .ok_or_else(|| fail("expected no style attribute when style Object is empty"))
}

#[test]
fn extract_preserves_parsed_style_through_round_trip() -> Result<(), Error> {
    let doc = run_and_extract(
        "<html><body><div id='host' style='color: red'></div></body></html>",
        "/* read-only */",
    )?;
    let (element, _) =
        find_element(doc.arena(), doc.root(), "div").ok_or_else(|| fail("div not found"))?;
    let style = style_attribute(element).ok_or_else(|| fail("style attribute missing"))?;
    (style == "color: red")
        .then_some(())
        .ok_or_else(|| fail("expected 'color: red' to round-trip unchanged"))
}

#[test]
fn extract_reflects_js_overwrites_of_parsed_style() -> Result<(), Error> {
    let doc = run_and_extract(
        "<html><body><div id='host' style='color: red'></div></body></html>",
        "document.getElementById('host').style.color = 'blue';",
    )?;
    let (element, _) =
        find_element(doc.arena(), doc.root(), "div").ok_or_else(|| fail("div not found"))?;
    let style = style_attribute(element).ok_or_else(|| fail("style attribute missing"))?;
    (style == "color: blue")
        .then_some(())
        .ok_or_else(|| fail("expected JS write to override parsed style on extract"))
}

#[test]
fn extract_appends_new_style_props_to_existing_parse() -> Result<(), Error> {
    let doc = run_and_extract(
        "<html><body><div id='host' style='color: red'></div></body></html>",
        "document.getElementById('host').style.padding = '5px';",
    )?;
    let (element, _) =
        find_element(doc.arena(), doc.root(), "div").ok_or_else(|| fail("div not found"))?;
    let style = style_attribute(element).ok_or_else(|| fail("style attribute missing"))?;
    style
        .contains("color: red")
        .then_some(())
        .ok_or_else(|| fail("expected parsed 'color: red' to persist"))?;
    style
        .contains("padding: 5px")
        .then_some(())
        .ok_or_else(|| fail("expected appended 'padding: 5px' to appear"))
}

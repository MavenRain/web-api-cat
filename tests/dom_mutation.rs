//! `document.createElement` + `Element.appendChild` (v0.6.1).

use boa_cat::env::Env;
use boa_cat::evaluate_program_with;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::value::Value;
use ecma_lex_cat::lex;
use ecma_parse_cat::parse_script;
use web_api_cat::Error;

fn run(html: &str, script: &str) -> Result<Value, Error> {
    let html_doc = html_cat::parse(html)?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let tokens = lex(script).map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, _heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(100_000)).map_err(Error::from)?;
    Ok(value)
}

fn fail(_msg: &'static str) -> Error {
    Error::Engine(boa_cat::Error::Unsupported { feature: "test" })
}

#[test]
fn create_element_returns_object_with_requested_tag() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "document.createElement('span').tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "span")
        .then_some(())
        .ok_or_else(|| fail("expected tagName 'span'"))
}

#[test]
fn create_element_has_empty_children_array() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "document.createElement('div').children.length",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected children.length === 0"))
}

#[test]
fn create_element_supports_setattribute_immediately() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const el = document.createElement('a'); el.setAttribute('href', '/x'); el.getAttribute('href')",
    )?;
    matches!(value, Value::String(ref s) if s == "/x")
        .then_some(())
        .ok_or_else(|| fail("expected href '/x' after setAttribute"))
}

#[test]
fn append_child_extends_parent_children_length() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.appendChild(document.createElement('span'));
        host.children.length",
    )?;
    matches!(value, Value::Number(n) if (n - 1.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected children.length === 1 after appendChild"))
}

#[test]
fn append_child_returns_appended_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        const child = document.createElement('span');
        host.appendChild(child).tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "span")
        .then_some(())
        .ok_or_else(|| fail("expected appendChild to return the child"))
}

#[test]
fn append_child_makes_child_reachable_via_children_index() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        const child = document.createElement('p');
        child.setAttribute('id', 'new-id');
        host.appendChild(child);
        host.children[0].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "p")
        .then_some(())
        .ok_or_else(|| fail("expected children[0].tagName === 'p'"))
}

#[test]
fn multiple_appendchild_calls_stack_in_order() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.appendChild(document.createElement('a'));
        host.appendChild(document.createElement('b'));
        host.appendChild(document.createElement('c'));
        host.children[0].tagName + host.children[1].tagName + host.children[2].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "abc")
        .then_some(())
        .ok_or_else(|| fail("expected appended children in order a/b/c"))
}

#[test]
fn append_child_preserves_existing_children() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p id='first'>existing</p></div></body></html>",
        "const host = document.getElementById('host');
        host.appendChild(document.createElement('span'));
        host.children.length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected children.length === 2 (kept existing + appended)"))
}

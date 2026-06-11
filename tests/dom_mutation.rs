//! `document.createElement` + `Element.appendChild` (v0.6.1),
//! `Element.removeChild` + `Element.insertBefore` (v0.6.2).

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

#[test]
fn remove_child_decrements_children_length() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p id='first'>x</p><span id='second'>y</span></div></body></html>",
        "const host = document.getElementById('host');
        const first = document.getElementById('first');
        host.removeChild(first);
        host.children.length",
    )?;
    matches!(value, Value::Number(n) if (n - 1.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected children.length === 1 after removeChild"))
}

#[test]
fn remove_child_returns_removed_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p id='first'>x</p></div></body></html>",
        "const host = document.getElementById('host');
        const first = document.getElementById('first');
        host.removeChild(first).tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "p")
        .then_some(())
        .ok_or_else(|| fail("expected removeChild to return the removed child"))
}

#[test]
fn remove_child_shifts_remaining_children_down() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p id='first'>x</p><span id='second'>y</span><a id='third'>z</a></div></body></html>",
        "const host = document.getElementById('host');
        const second = document.getElementById('second');
        host.removeChild(second);
        host.children[0].tagName + host.children[1].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "pa")
        .then_some(())
        .ok_or_else(|| fail("expected children to shift to [p, a] after removing middle span"))
}

#[test]
fn remove_child_noop_on_non_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p id='first'>x</p></div></body></html>",
        "const host = document.getElementById('host');
        const stranger = document.createElement('span');
        host.removeChild(stranger);
        host.children.length",
    )?;
    matches!(value, Value::Number(n) if (n - 1.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| {
            fail("expected children.length === 1 (no-op when child is not actually a child)")
        })
}

#[test]
fn insert_before_null_reference_appends() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p id='first'>x</p></div></body></html>",
        "const host = document.getElementById('host');
        host.insertBefore(document.createElement('span'), null);
        host.children[1].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "span")
        .then_some(())
        .ok_or_else(|| fail("expected insertBefore(_, null) to append at end"))
}

#[test]
fn insert_before_returns_new_node() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p id='ref'>x</p></div></body></html>",
        "const host = document.getElementById('host');
        const ref_ = document.getElementById('ref');
        host.insertBefore(document.createElement('span'), ref_).tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "span")
        .then_some(())
        .ok_or_else(|| fail("expected insertBefore to return the inserted node"))
}

#[test]
fn insert_before_places_node_at_reference_index() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p id='first'>x</p><a id='second'>y</a></div></body></html>",
        "const host = document.getElementById('host');
        const second = document.getElementById('second');
        host.insertBefore(document.createElement('span'), second);
        host.children[0].tagName + host.children[1].tagName + host.children[2].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "pspana")
        .then_some(())
        .ok_or_else(|| {
            fail("expected children to become [p, span, a] after insertBefore at index 1")
        })
}

#[test]
fn insert_before_at_first_position_shifts_all_children_up() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p id='first'>x</p><a id='second'>y</a></div></body></html>",
        "const host = document.getElementById('host');
        const first = document.getElementById('first');
        host.insertBefore(document.createElement('span'), first);
        host.children[0].tagName + host.children[1].tagName + host.children[2].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "spanpa")
        .then_some(())
        .ok_or_else(|| {
            fail("expected children to become [span, p, a] after insertBefore at index 0")
        })
}

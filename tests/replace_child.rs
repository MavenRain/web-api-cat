//! `Element.replaceChild(newChild, oldChild)` (v0.6.7): swap one
//! child in the parent's children array, preserve length.

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
        evaluate_program_with(&program, env, heap, Fuel::new(200_000)).map_err(Error::from)?;
    Ok(value)
}

fn fail(_msg: &'static str) -> Error {
    Error::Engine(boa_cat::Error::Unsupported { feature: "test" })
}

#[test]
fn replace_child_returns_old_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span id='old'>x</span></div></body></html>",
        "const host = document.getElementById('host');
        const old = document.getElementById('old');
        host.replaceChild(document.createElement('a'), old).tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "span")
        .then_some(())
        .ok_or_else(|| fail("expected replaceChild to return the replaced (old) child"))
}

#[test]
fn replace_child_swaps_in_place() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span id='old'>x</span></div></body></html>",
        "const host = document.getElementById('host');
        const old = document.getElementById('old');
        host.replaceChild(document.createElement('a'), old);
        host.children[0].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "a")
        .then_some(())
        .ok_or_else(|| fail("expected children[0] to be the new child after replaceChild"))
}

#[test]
fn replace_child_preserves_length() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>1</a><b>2</b><c>3</c></div></body></html>",
        "const host = document.getElementById('host');
        const second = host.children[1];
        host.replaceChild(document.createElement('z'), second);
        host.children.length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected children.length unchanged after replaceChild"))
}

#[test]
fn replace_child_keeps_siblings_at_their_indexes() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>1</a><b>2</b><c>3</c></div></body></html>",
        "const host = document.getElementById('host');
        const middle = host.children[1];
        host.replaceChild(document.createElement('z'), middle);
        host.children[0].tagName + host.children[1].tagName + host.children[2].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "azc")
        .then_some(())
        .ok_or_else(|| fail("expected children to become [a, z, c]"))
}

#[test]
fn replace_child_noop_on_non_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span>x</span></div></body></html>",
        "const host = document.getElementById('host');
        const stranger = document.createElement('p');
        host.replaceChild(document.createElement('z'), stranger);
        host.children[0].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "span")
        .then_some(())
        .ok_or_else(|| fail("expected no-op when oldChild is not actually a child"))
}

#[test]
fn replace_child_works_on_created_element() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const host = document.createElement('div');
        const old_child = document.createElement('span');
        host.appendChild(old_child);
        host.replaceChild(document.createElement('p'), old_child);
        host.children[0].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "p")
        .then_some(())
        .ok_or_else(|| fail("expected replaceChild to work on createElement output"))
}

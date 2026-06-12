//! `Element.cloneNode(deep)` (v0.6.6): shallow + deep clones that
//! independently own `__attributes`, `style`, `children`, and
//! `classList`.

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
fn clone_node_shallow_keeps_tag_name() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span>x</span></div></body></html>",
        "document.getElementById('host').cloneNode().tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "div")
        .then_some(())
        .ok_or_else(|| fail("expected shallow clone to keep tagName"))
}

#[test]
fn clone_node_shallow_drops_children() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span>x</span><span>y</span></div></body></html>",
        "document.getElementById('host').cloneNode().children.length",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected shallow clone to have empty children"))
}

#[test]
fn clone_node_shallow_preserves_attributes() -> Result<(), Error> {
    let value = run(
        "<html><body><a id='host' href='/x'>go</a></body></html>",
        "document.getElementById('host').cloneNode().getAttribute('href')",
    )?;
    matches!(value, Value::String(ref s) if s == "/x")
        .then_some(())
        .ok_or_else(|| fail("expected shallow clone to keep href attribute"))
}

#[test]
fn clone_node_deep_clones_descendants() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span><a>x</a></span></div></body></html>",
        "const clone = document.getElementById('host').cloneNode(true);
        clone.children[0].children[0].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "a")
        .then_some(())
        .ok_or_else(|| fail("expected deep clone to preserve grandchild tagName"))
}

#[test]
fn clone_node_deep_preserves_children_length() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>1</a><b>2</b><c>3</c></div></body></html>",
        "document.getElementById('host').cloneNode(true).children.length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected deep clone to keep 3 children"))
}

#[test]
fn clone_node_attributes_are_independent_of_original() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' data-x='one'></div></body></html>",
        "const host = document.getElementById('host');
        const clone = host.cloneNode();
        clone.setAttribute('data-x', 'two');
        host.getAttribute('data-x') + ',' + clone.getAttribute('data-x')",
    )?;
    matches!(value, Value::String(ref s) if s == "one,two")
        .then_some(())
        .ok_or_else(|| fail("expected attribute writes on clone to not bleed into original"))
}

#[test]
fn clone_node_class_list_is_independent_of_original() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='alpha'></div></body></html>",
        "const host = document.getElementById('host');
        const clone = host.cloneNode();
        clone.classList.add('beta');
        host.className + ',' + clone.className",
    )?;
    matches!(value, Value::String(ref s) if s == "alpha,alpha beta")
        .then_some(())
        .ok_or_else(|| fail("expected classList writes on clone to not bleed into original"))
}

#[test]
fn clone_node_style_is_independent_of_original() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.style.color = 'red';
        const clone = host.cloneNode();
        clone.style.color = 'blue';
        host.style.color + ',' + clone.style.color",
    )?;
    matches!(value, Value::String(ref s) if s == "red,blue")
        .then_some(())
        .ok_or_else(|| fail("expected style writes on clone to not bleed into original"))
}

#[test]
fn clone_node_deep_children_are_independent_of_original() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span id='leaf'>x</span></div></body></html>",
        "const host = document.getElementById('host');
        const clone = host.cloneNode(true);
        clone.children[0].setAttribute('data-touched', 'yes');
        const original_leaf = document.getElementById('leaf');
        const cloned_leaf = clone.children[0];
        const original_has = original_leaf.hasAttribute('data-touched');
        const cloned_has = cloned_leaf.hasAttribute('data-touched');
        (original_has === false && cloned_has === true) ? 'isolated' : 'shared'",
    )?;
    matches!(value, Value::String(ref s) if s == "isolated")
        .then_some(())
        .ok_or_else(|| fail("expected deep clone's children to be independent of originals"))
}

#[test]
fn clone_node_works_on_created_element() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const el = document.createElement('p');
        el.setAttribute('id', 'src');
        el.cloneNode().tagName + ',' + el.cloneNode().getAttribute('id')",
    )?;
    matches!(value, Value::String(ref s) if s == "p,src")
        .then_some(())
        .ok_or_else(|| fail("expected cloneNode to work on createElement output"))
}

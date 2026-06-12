//! `Element.firstElementChild` / `lastElementChild` /
//! `previousElementSibling` / `nextElementSibling` (v0.6.9).

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
fn first_element_child_returns_first_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>1</a><b>2</b><c>3</c></div></body></html>",
        "document.getElementById('host').firstElementChild.tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "a")
        .then_some(())
        .ok_or_else(|| fail("expected firstElementChild to be the first child"))
}

#[test]
fn first_element_child_is_null_when_no_children() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "document.getElementById('host').firstElementChild === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected firstElementChild to be null for empty element"))
}

#[test]
fn last_element_child_returns_last_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>1</a><b>2</b><c>3</c></div></body></html>",
        "document.getElementById('host').lastElementChild.tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "c")
        .then_some(())
        .ok_or_else(|| fail("expected lastElementChild to be the last child"))
}

#[test]
fn last_element_child_is_null_when_no_children() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "document.getElementById('host').lastElementChild === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected lastElementChild to be null for empty element"))
}

#[test]
fn previous_element_sibling_returns_preceding_sibling() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>1</a><b id='target'>2</b><c>3</c></div></body></html>",
        "document.getElementById('target').previousElementSibling.tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "a")
        .then_some(())
        .ok_or_else(|| fail("expected previousElementSibling to be the preceding child"))
}

#[test]
fn previous_element_sibling_is_null_for_first_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a id='target'>1</a><b>2</b></div></body></html>",
        "document.getElementById('target').previousElementSibling === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected previousElementSibling to be null for first child"))
}

#[test]
fn next_element_sibling_returns_following_sibling() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>1</a><b id='target'>2</b><c>3</c></div></body></html>",
        "document.getElementById('target').nextElementSibling.tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "c")
        .then_some(())
        .ok_or_else(|| fail("expected nextElementSibling to be the following child"))
}

#[test]
fn next_element_sibling_is_null_for_last_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>1</a><b id='target'>2</b></div></body></html>",
        "document.getElementById('target').nextElementSibling === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected nextElementSibling to be null for last child"))
}

#[test]
fn sibling_is_null_for_detached_element() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const el = document.createElement('span');
        el.previousElementSibling === null && el.nextElementSibling === null ? 'both-null' : 'attached'",
    )?;
    matches!(value, Value::String(ref s) if s == "both-null")
        .then_some(())
        .ok_or_else(|| fail("expected both siblings to be null for detached element"))
}

#[test]
fn siblings_update_after_append_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        const a = document.createElement('a');
        const b = document.createElement('b');
        host.appendChild(a);
        host.appendChild(b);
        a.nextElementSibling.tagName + ',' + b.previousElementSibling.tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "b,a")
        .then_some(())
        .ok_or_else(|| fail("expected siblings to reflect post-append children-array state"))
}

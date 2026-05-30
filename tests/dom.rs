//! Integration tests covering document, element, and fetch bindings.

use boa_cat::env::Env;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::{Value, evaluate_program_with};
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
fn get_element_by_id_returns_element() -> Result<(), Error> {
    let value = run(
        "<html><body><p id='greet'>hello</p></body></html>",
        "document.getElementById('greet').textContent",
    )?;
    matches!(value, Value::String(ref s) if s == "hello")
        .then_some(())
        .ok_or_else(|| fail("expected 'hello'"))
}

#[test]
fn get_element_by_id_missing_returns_null() -> Result<(), Error> {
    let value = run(
        "<html><body><p>hi</p></body></html>",
        "document.getElementById('missing')",
    )?;
    matches!(value, Value::Null)
        .then_some(())
        .ok_or_else(|| fail("expected null for missing id"))
}

#[test]
fn query_selector_tag() -> Result<(), Error> {
    let value = run(
        "<html><body><span>x</span><p>y</p></body></html>",
        "document.querySelector('p').textContent",
    )?;
    matches!(value, Value::String(ref s) if s == "y")
        .then_some(())
        .ok_or_else(|| fail("expected 'y' for p match"))
}

#[test]
fn query_selector_class() -> Result<(), Error> {
    let value = run(
        "<html><body><p class='a'>1</p><p class='b'>2</p></body></html>",
        "document.querySelector('.b').textContent",
    )?;
    matches!(value, Value::String(ref s) if s == "2")
        .then_some(())
        .ok_or_else(|| fail("expected '2' for .b match"))
}

#[test]
fn element_tag_name_lowercased() -> Result<(), Error> {
    let value = run(
        "<html><body><P id='p'>hi</P></body></html>",
        "document.getElementById('p').tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "p")
        .then_some(())
        .ok_or_else(|| fail("expected lowercase tagName"))
}

#[test]
fn element_class_name() -> Result<(), Error> {
    let value = run(
        "<html><body><p id='p' class='greeting big'>hi</p></body></html>",
        "document.getElementById('p').className",
    )?;
    matches!(value, Value::String(ref s) if s == "greeting big")
        .then_some(())
        .ok_or_else(|| fail("expected className 'greeting big'"))
}

#[test]
fn get_attribute_returns_value() -> Result<(), Error> {
    let value = run(
        "<html><body><a id='link' href='https://example.com'>x</a></body></html>",
        "document.getElementById('link').getAttribute('href')",
    )?;
    matches!(value, Value::String(ref s) if s == "https://example.com")
        .then_some(())
        .ok_or_else(|| fail("expected href"))
}

#[test]
fn get_attribute_missing_returns_null() -> Result<(), Error> {
    let value = run(
        "<html><body><p id='p'>x</p></body></html>",
        "document.getElementById('p').getAttribute('missing')",
    )?;
    matches!(value, Value::Null)
        .then_some(())
        .ok_or_else(|| fail("expected null for missing attr"))
}

#[test]
fn has_attribute_true_false() -> Result<(), Error> {
    let value = run(
        "<html><body><input id='i' disabled></body></html>",
        "const el = document.getElementById('i'); el.hasAttribute('disabled')",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected true for disabled"))
}

#[test]
fn set_attribute_round_trip() -> Result<(), Error> {
    let value = run(
        "<html><body><p id='p'>x</p></body></html>",
        "const el = document.getElementById('p'); el.setAttribute('data-x', '42'); el.getAttribute('data-x')",
    )?;
    matches!(value, Value::String(ref s) if s == "42")
        .then_some(())
        .ok_or_else(|| fail("expected '42' after setAttribute"))
}

#[test]
fn body_alias_resolves() -> Result<(), Error> {
    let value = run(
        "<html><body><p>x</p></body></html>",
        "document.body.tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "body")
        .then_some(())
        .ok_or_else(|| fail("expected body tagName"))
}

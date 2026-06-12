//! `Element.innerHTML` (v0.6.5): accessor getter that serialises
//! `this.children` to HTML, and accessor setter that parses an
//! HTML fragment and replaces `this.children` with the parse
//! result.

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
fn inner_html_get_serializes_single_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span>hi</span></div></body></html>",
        "document.getElementById('host').innerHTML",
    )?;
    matches!(value, Value::String(ref s) if s == "<span>hi</span>")
        .then_some(())
        .ok_or_else(|| fail("expected innerHTML === '<span>hi</span>'"))
}

#[test]
fn inner_html_get_serializes_multiple_children_in_order() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>x</a><b>y</b><c>z</c></div></body></html>",
        "document.getElementById('host').innerHTML",
    )?;
    matches!(value, Value::String(ref s) if s == "<a>x</a><b>y</b><c>z</c>")
        .then_some(())
        .ok_or_else(|| fail("expected innerHTML to serialise children in order"))
}

#[test]
fn inner_html_get_includes_attributes() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a href='/x' id='link'>go</a></div></body></html>",
        "document.getElementById('host').innerHTML",
    )?;
    let Value::String(ref s) = value else {
        return Err(fail("expected innerHTML to be a string"));
    };
    s.contains(" href=\"/x\"")
        .then_some(())
        .ok_or_else(|| fail("expected innerHTML to include href attribute"))
}

#[test]
fn inner_html_get_escapes_text_content() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.textContent = '<script>x</script>';
        host.innerHTML",
    )?;
    matches!(value, Value::String(ref s) if s == "&lt;script&gt;x&lt;/script&gt;")
        .then_some(())
        .ok_or_else(|| fail("expected innerHTML to HTML-escape textContent"))
}

#[test]
fn inner_html_get_on_leaf_uses_text_content() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'>plain text</div></body></html>",
        "document.getElementById('host').innerHTML",
    )?;
    matches!(value, Value::String(ref s) if s == "plain text")
        .then_some(())
        .ok_or_else(|| fail("expected leaf innerHTML to be the (escaped) text content"))
}

#[test]
fn inner_html_set_replaces_children_with_parsed_fragment() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span>old</span></div></body></html>",
        "const host = document.getElementById('host');
        host.innerHTML = '<p>new</p>';
        host.children[0].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "p")
        .then_some(())
        .ok_or_else(|| fail("expected first child tagName to be 'p' after innerHTML set"))
}

#[test]
fn inner_html_set_updates_children_length() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span>old</span></div></body></html>",
        "const host = document.getElementById('host');
        host.innerHTML = '<a>1</a><b>2</b><c>3</c>';
        host.children.length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected children.length === 3 after innerHTML set"))
}

#[test]
fn inner_html_set_empty_string_clears_children() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span>a</span><span>b</span></div></body></html>",
        "const host = document.getElementById('host');
        host.innerHTML = '';
        host.children.length",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected innerHTML = '' to clear children"))
}

#[test]
fn inner_html_round_trip_through_set_and_get() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.innerHTML = '<p>round</p>';
        host.innerHTML",
    )?;
    matches!(value, Value::String(ref s) if s == "<p>round</p>")
        .then_some(())
        .ok_or_else(|| fail("expected innerHTML round-trip"))
}

#[test]
fn inner_html_works_on_created_element() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const el = document.createElement('div');
        el.innerHTML = '<span>x</span>';
        el.children[0].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "span")
        .then_some(())
        .ok_or_else(|| fail("expected innerHTML to work on createElement output"))
}

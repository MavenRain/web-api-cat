//! `Element.style` (v0.6.4): empty Object on each element; scripts
//! read and write arbitrary keys through the engine's normal
//! property storage.

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
fn style_is_an_object_on_parsed_element() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "typeof document.getElementById('host').style",
    )?;
    matches!(value, Value::String(ref s) if s == "object")
        .then_some(())
        .ok_or_else(|| fail("expected typeof style === 'object'"))
}

#[test]
fn style_property_assignment_round_trips() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.style.color = 'red';
        host.style.color",
    )?;
    matches!(value, Value::String(ref s) if s == "red")
        .then_some(())
        .ok_or_else(|| fail("expected style.color to round-trip"))
}

#[test]
fn multiple_style_properties_coexist() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.style.color = 'red';
        host.style.display = 'none';
        host.style.color + ',' + host.style.display",
    )?;
    matches!(value, Value::String(ref s) if s == "red,none")
        .then_some(())
        .ok_or_else(|| fail("expected two style props to coexist"))
}

#[test]
fn style_unset_property_is_undefined() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "typeof document.getElementById('host').style.color",
    )?;
    matches!(value, Value::String(ref s) if s == "undefined")
        .then_some(())
        .ok_or_else(|| fail("expected unset style property to be undefined"))
}

#[test]
fn style_works_on_created_element() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const el = document.createElement('p');
        el.style.fontSize = '14px';
        el.style.fontSize",
    )?;
    matches!(value, Value::String(ref s) if s == "14px")
        .then_some(())
        .ok_or_else(|| fail("expected style on createElement output"))
}

#[test]
fn style_objects_are_per_element_not_shared() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='a'></div><div id='b'></div></body></html>",
        "const a = document.getElementById('a');
        const b = document.getElementById('b');
        a.style.color = 'red';
        b.style.color = 'blue';
        a.style.color + ',' + b.style.color",
    )?;
    matches!(value, Value::String(ref s) if s == "red,blue")
        .then_some(())
        .ok_or_else(|| fail("expected style objects to be per-element, not shared"))
}

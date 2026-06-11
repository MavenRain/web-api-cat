//! `Element.classList` (v0.6.3): `add`, `remove`, `contains`,
//! `toggle`.

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
fn class_list_contains_reads_parsed_class_attribute() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='alpha beta'></div></body></html>",
        "document.getElementById('host').classList.contains('alpha')",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected classList.contains('alpha') to be true on parsed element"))
}

#[test]
fn class_list_contains_returns_false_for_missing_token() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='alpha'></div></body></html>",
        "document.getElementById('host').classList.contains('beta')",
    )?;
    matches!(value, Value::Boolean(false))
        .then_some(())
        .ok_or_else(|| fail("expected classList.contains('beta') to be false"))
}

#[test]
fn class_list_add_updates_class_name() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='alpha'></div></body></html>",
        "const host = document.getElementById('host');
        host.classList.add('beta');
        host.className",
    )?;
    matches!(value, Value::String(ref s) if s == "alpha beta")
        .then_some(())
        .ok_or_else(|| fail("expected className === 'alpha beta' after classList.add"))
}

#[test]
fn class_list_add_does_not_duplicate_existing_token() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='alpha'></div></body></html>",
        "const host = document.getElementById('host');
        host.classList.add('alpha');
        host.className",
    )?;
    matches!(value, Value::String(ref s) if s == "alpha")
        .then_some(())
        .ok_or_else(|| fail("expected className unchanged when adding existing token"))
}

#[test]
fn class_list_remove_drops_token_and_updates_get_attribute() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='alpha beta gamma'></div></body></html>",
        "const host = document.getElementById('host');
        host.classList.remove('beta');
        host.getAttribute('class')",
    )?;
    matches!(value, Value::String(ref s) if s == "alpha gamma")
        .then_some(())
        .ok_or_else(|| fail("expected getAttribute('class') === 'alpha gamma' after remove"))
}

#[test]
fn class_list_toggle_adds_missing_token_and_returns_true() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='alpha'></div></body></html>",
        "document.getElementById('host').classList.toggle('beta')",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected toggle to return true after adding new token"))
}

#[test]
fn class_list_toggle_removes_existing_token_and_returns_false() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='alpha beta'></div></body></html>",
        "document.getElementById('host').classList.toggle('alpha')",
    )?;
    matches!(value, Value::Boolean(false))
        .then_some(())
        .ok_or_else(|| fail("expected toggle to return false after removing existing token"))
}

#[test]
fn class_list_works_on_created_element() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const el = document.createElement('span');
        el.classList.add('one');
        el.classList.add('two');
        el.className",
    )?;
    matches!(value, Value::String(ref s) if s == "one two")
        .then_some(())
        .ok_or_else(|| fail("expected classList to work on createElement output"))
}

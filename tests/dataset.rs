//! `Element.dataset` (v0.7.7): snapshot of `data-*` attributes
//! as camelCase keys.

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
fn single_segment_data_attribute_reachable_via_dataset() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' data-foo='bar'></div></body></html>",
        "document.getElementById('host').dataset.foo",
    )?;
    matches!(value, Value::String(ref s) if s == "bar")
        .then_some(())
        .ok_or_else(|| fail("expected dataset.foo === 'bar'"))
}

#[test]
fn two_segment_data_attribute_camelcases() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' data-user-name='alice'></div></body></html>",
        "document.getElementById('host').dataset.userName",
    )?;
    matches!(value, Value::String(ref s) if s == "alice")
        .then_some(())
        .ok_or_else(|| fail("expected data-user-name -> dataset.userName"))
}

#[test]
fn three_segment_data_attribute_camelcases() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' data-user-profile-id='42'></div></body></html>",
        "document.getElementById('host').dataset.userProfileId",
    )?;
    matches!(value, Value::String(ref s) if s == "42")
        .then_some(())
        .ok_or_else(|| fail("expected data-user-profile-id -> dataset.userProfileId"))
}

#[test]
fn multiple_data_attributes_coexist_in_dataset() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' data-id='1' data-name='alice' data-role='admin'></div></body></html>",
        "const host = document.getElementById('host');
        host.dataset.id + ',' + host.dataset.name + ',' + host.dataset.role",
    )?;
    matches!(value, Value::String(ref s) if s == "1,alice,admin")
        .then_some(())
        .ok_or_else(|| fail("expected three dataset properties to coexist"))
}

#[test]
fn non_data_attributes_are_excluded_from_dataset() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='foo' style='color: red' data-x='y'></div></body></html>",
        "const host = document.getElementById('host');
        const has_class = typeof host.dataset.class;
        const has_style = typeof host.dataset.style;
        const has_x = host.dataset.x;
        has_class + ',' + has_style + ',' + has_x",
    )?;
    matches!(value, Value::String(ref s) if s == "undefined,undefined,y")
        .then_some(())
        .ok_or_else(|| fail("expected only data-* attributes to surface in dataset"))
}

#[test]
fn bare_data_prefix_is_not_in_dataset() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' data='nope' data-foo='yes'></div></body></html>",
        "const host = document.getElementById('host');
        typeof host.dataset[''] + ',' + host.dataset.foo",
    )?;
    matches!(value, Value::String(ref s) if s == "undefined,yes")
        .then_some(())
        .ok_or_else(|| fail("expected bare 'data' attribute to not become a dataset entry"))
}

#[test]
fn dataset_on_created_element_starts_empty() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const el = document.createElement('div');
        typeof el.dataset",
    )?;
    matches!(value, Value::String(ref s) if s == "object")
        .then_some(())
        .ok_or_else(|| fail("expected createElement output to have an empty dataset object"))
}

#[test]
fn dataset_writes_round_trip_within_the_object() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.dataset.newKey = 'v';
        host.dataset.newKey",
    )?;
    matches!(value, Value::String(ref s) if s == "v")
        .then_some(())
        .ok_or_else(|| fail("expected dataset writes to round-trip through the object"))
}

#[test]
fn dataset_objects_are_per_element_not_shared() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='a' data-x='ax'></div><div id='b' data-x='bx'></div></body></html>",
        "const a = document.getElementById('a');
        const b = document.getElementById('b');
        a.dataset.x + ',' + b.dataset.x",
    )?;
    matches!(value, Value::String(ref s) if s == "ax,bx")
        .then_some(())
        .ok_or_else(|| fail("expected dataset to be per-element, not shared"))
}

#[test]
fn dataset_value_is_a_string_even_when_attribute_looks_numeric() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' data-count='42'></div></body></html>",
        "typeof document.getElementById('host').dataset.count",
    )?;
    matches!(value, Value::String(ref s) if s == "string")
        .then_some(())
        .ok_or_else(|| fail("expected dataset values to be strings per spec"))
}

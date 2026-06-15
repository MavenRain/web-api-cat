//! v0.7.18 form-control accessors: `<input>.value` /
//! `<input>.checked` property reflection and the `:checked`
//! pseudo-class.

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
fn input_value_getter_reflects_attribute() -> Result<(), Error> {
    let value = run(
        "<html><body><input id='host' value='initial'/></body></html>",
        "document.getElementById('host').value",
    )?;
    matches!(value, Value::String(ref s) if s == "initial")
        .then_some(())
        .ok_or_else(|| fail("expected input.value to reflect the value attribute"))
}

#[test]
fn input_value_getter_empty_when_attribute_missing() -> Result<(), Error> {
    let value = run(
        "<html><body><input id='host'/></body></html>",
        "document.getElementById('host').value",
    )?;
    matches!(value, Value::String(ref s) if s.is_empty())
        .then_some(())
        .ok_or_else(|| fail("expected input.value to default to ''"))
}

#[test]
fn input_value_setter_round_trips() -> Result<(), Error> {
    let value = run(
        "<html><body><input id='host' value='before'/></body></html>",
        "const el = document.getElementById('host');
        el.value = 'after';
        el.value",
    )?;
    matches!(value, Value::String(ref s) if s == "after")
        .then_some(())
        .ok_or_else(|| fail("expected input.value setter to round-trip"))
}

#[test]
fn input_value_setter_updates_get_attribute() -> Result<(), Error> {
    // Setting .value should also be visible via getAttribute('value').
    let value = run(
        "<html><body><input id='host' value='before'/></body></html>",
        "const el = document.getElementById('host');
        el.value = 'after';
        el.getAttribute('value')",
    )?;
    matches!(value, Value::String(ref s) if s == "after")
        .then_some(())
        .ok_or_else(|| fail("expected input.value setter to reflect via getAttribute"))
}

#[test]
fn input_checked_getter_reflects_attribute_presence() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <input id='on' type='checkbox' checked/>
            <input id='off' type='checkbox'/>
        </body></html>",
        "const on = document.getElementById('on');
        const off = document.getElementById('off');
        const ok = on.checked === true && off.checked === false;
        ok ? 'ok' : 'wrong'",
    )?;
    matches!(value, Value::String(ref s) if s == "ok")
        .then_some(())
        .ok_or_else(|| fail("expected input.checked to reflect attribute presence"))
}

#[test]
fn input_checked_setter_adds_and_removes_attribute() -> Result<(), Error> {
    let value = run(
        "<html><body><input id='host' type='checkbox'/></body></html>",
        "const el = document.getElementById('host');
        el.checked = true;
        const after_on = el.hasAttribute('checked');
        el.checked = false;
        const after_off = el.hasAttribute('checked');
        (after_on === true && after_off === false) ? 'ok' : 'wrong'",
    )?;
    matches!(value, Value::String(ref s) if s == "ok")
        .then_some(())
        .ok_or_else(|| fail("expected input.checked setter to add then remove the attribute"))
}

#[test]
fn checked_pseudo_class_via_query_selector() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <input type='checkbox'/>
            <input id='want' type='checkbox' checked/>
            <input type='checkbox'/>
        </body></html>",
        "document.querySelector(':checked').id",
    )?;
    matches!(value, Value::String(ref s) if s == "want")
        .then_some(())
        .ok_or_else(|| fail("expected ':checked' to pick the checked input"))
}

#[test]
fn checked_pseudo_class_via_query_selector_all() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <input type='checkbox' checked/>
            <input type='checkbox'/>
            <input type='checkbox' checked/>
            <input type='radio' checked/>
        </body></html>",
        "document.querySelectorAll(':checked').length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected three :checked inputs"))
}

#[test]
fn checked_pseudo_class_combines_with_tag_and_attribute() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <input type='checkbox' checked/>
            <input id='want' type='radio' checked name='group'/>
            <input type='radio' name='group'/>
        </body></html>",
        "document.querySelector('input[type=radio]:checked').id",
    )?;
    matches!(value, Value::String(ref s) if s == "want")
        .then_some(())
        .ok_or_else(|| fail("expected 'input[type=radio]:checked' to pick the checked radio"))
}

#[test]
fn checked_pseudo_class_updates_after_setter() -> Result<(), Error> {
    // Set checked via the JS property, then query-select :checked --
    // the new state should be visible.
    let value = run(
        "<html><body>
            <input id='a' type='checkbox'/>
            <input id='b' type='checkbox'/>
        </body></html>",
        "document.getElementById('b').checked = true;
        document.querySelector(':checked').id",
    )?;
    matches!(value, Value::String(ref s) if s == "b")
        .then_some(())
        .ok_or_else(|| fail("expected ':checked' to track the post-setter state"))
}

#[test]
fn checked_pseudo_class_combines_with_not() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <input id='want' type='checkbox'/>
            <input type='checkbox' checked/>
        </body></html>",
        "document.querySelector('input:not(:checked)').id",
    )?;
    matches!(value, Value::String(ref s) if s == "want")
        .then_some(())
        .ok_or_else(|| fail("expected ':not(:checked)' to pick the unchecked input"))
}

#[test]
fn matches_with_checked_pseudo_class() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <input id='on' type='checkbox' checked/>
            <input id='off' type='checkbox'/>
        </body></html>",
        "const on = document.getElementById('on');
        const off = document.getElementById('off');
        const ok = on.matches(':checked') === true && off.matches(':checked') === false;
        ok ? 'ok' : 'wrong'",
    )?;
    matches!(value, Value::String(ref s) if s == "ok")
        .then_some(())
        .ok_or_else(|| fail("expected matches(':checked') to discriminate by attribute"))
}

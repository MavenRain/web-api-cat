//! `new Event(type, options)` / `new CustomEvent(type, options)`
//! constructors (v0.7.9), plus the in-place dispatch decoration
//! that lets scripts inspect `event.defaultPrevented` on their
//! own reference after dispatch.

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
fn new_event_returns_object_with_type() -> Result<(), Error> {
    let value = run("<html><body></body></html>", "new Event('click').type")?;
    matches!(value, Value::String(ref s) if s == "click")
        .then_some(())
        .ok_or_else(|| fail("expected new Event('click').type === 'click'"))
}

#[test]
fn new_event_defaults_bubbles_cancelable_composed_to_false() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const e = new Event('click');
        e.bubbles === false && e.cancelable === false && e.composed === false
            ? 'all-false' : 'wrong'",
    )?;
    matches!(value, Value::String(ref s) if s == "all-false")
        .then_some(())
        .ok_or_else(|| fail("expected all three init flags to default to false"))
}

#[test]
fn new_event_honours_bubbles_option() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "new Event('click', { bubbles: true }).bubbles",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected { bubbles: true } to set bubbles=true"))
}

#[test]
fn new_event_honours_cancelable_option() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "new Event('click', { cancelable: true }).cancelable",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected { cancelable: true } to set cancelable=true"))
}

#[test]
fn new_event_default_prevented_starts_false() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "new Event('click').defaultPrevented",
    )?;
    matches!(value, Value::Boolean(false))
        .then_some(())
        .ok_or_else(|| fail("expected defaultPrevented to start false"))
}

#[test]
fn event_without_new_also_returns_event_object() -> Result<(), Error> {
    // boa-cat's construct fn calls the NativeFn; without `new`,
    // the engine calls it as a regular function.  Either way our
    // NativeFn returns an event-shaped Object.
    let value = run("<html><body></body></html>", "Event('keypress').type")?;
    matches!(value, Value::String(ref s) if s == "keypress")
        .then_some(())
        .ok_or_else(|| fail("expected bare Event(type) call to also yield an event"))
}

#[test]
fn dispatching_new_event_fires_listeners() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let count = 0;
        const host = document.getElementById('host');
        host.addEventListener('click', () => { count = count + 1; });
        host.dispatchEvent(new Event('click'));
        count",
    )?;
    matches!(value, Value::Number(n) if (n - 1.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected listener to fire on new Event dispatch"))
}

#[test]
fn user_reference_sees_default_prevented_after_dispatch() -> Result<(), Error> {
    // v0.7.9 in-place decoration: the user's `e` reference
    // sees the post-dispatch state of `defaultPrevented`.  This
    // is the spec-compliant behaviour scripts expect.
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.addEventListener('click', (e) => { e.preventDefault(); });
        const e = new Event('click');
        host.dispatchEvent(e);
        e.defaultPrevented",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| {
            fail("expected user-held event reference to see defaultPrevented after dispatch")
        })
}

#[test]
fn user_reference_sees_target_after_dispatch() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'>x</div></body></html>",
        "const host = document.getElementById('host');
        const e = new Event('click');
        host.dispatchEvent(e);
        e.target.tagName",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("div"))
        .then_some(())
        .ok_or_else(|| fail("expected user-held reference to see target after dispatch"))
}

#[test]
fn dispatch_resets_default_prevented_at_entry() -> Result<(), Error> {
    // v0.7.9 in-place decoration always resets defaultPrevented
    // to false at dispatch entry, so a pre-dispatch
    // `e.preventDefault()` is shadowed by the listener pass
    // through the bubble walk.  If no listener calls
    // preventDefault during dispatch, the final flag is false.
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.addEventListener('click', () => {});
        const e = new Event('click');
        e.preventDefault();
        // before dispatch: defaultPrevented is true
        const before = e.defaultPrevented;
        host.dispatchEvent(e);
        // after dispatch with no preventDefault-calling listener:
        // defaultPrevented is reset to false at decoration entry.
        const after = e.defaultPrevented;
        before === true && after === false ? 'reset' : 'not-reset'",
    )?;
    matches!(value, Value::String(ref s) if s == "reset")
        .then_some(())
        .ok_or_else(|| fail("expected dispatch to reset defaultPrevented at entry"))
}

#[test]
fn new_custom_event_carries_detail() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "new CustomEvent('foo', { detail: { x: 42 } }).detail.x",
    )?;
    matches!(value, Value::Number(n) if (n - 42.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected CustomEvent detail to surface the inner Object"))
}

#[test]
fn new_custom_event_detail_defaults_to_null() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const e = new CustomEvent('foo');
        e.detail === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected CustomEvent detail to default to null"))
}

#[test]
fn dispatching_custom_event_delivers_detail_to_handler() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let received = '';
        const host = document.getElementById('host');
        host.addEventListener('greet', (e) => { received = e.detail.name; });
        host.dispatchEvent(new CustomEvent('greet', { detail: { name: 'alice' } }));
        received",
    )?;
    matches!(value, Value::String(ref s) if s == "alice")
        .then_some(())
        .ok_or_else(|| fail("expected listener to read detail.name from CustomEvent"))
}

#[test]
fn new_event_object_has_method_bindings() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const e = new Event('click');
        typeof e.preventDefault + ',' + typeof e.stopPropagation + ',' + typeof e.stopImmediatePropagation",
    )?;
    matches!(value, Value::String(ref s) if s == "function,function,function")
        .then_some(())
        .ok_or_else(|| {
            fail("expected new Event to expose preventDefault/stopPropagation/stopImmediatePropagation")
        })
}

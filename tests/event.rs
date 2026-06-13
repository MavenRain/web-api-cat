//! `addEventListener` / `removeEventListener` / `dispatchEvent`
//! (v0.7.4): `EventTarget` mixin on every element, bubble dispatch
//! up the `__parent__` chain.

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
        evaluate_program_with(&program, env, heap, Fuel::new(500_000)).map_err(Error::from)?;
    Ok(value)
}

fn fail(_msg: &'static str) -> Error {
    Error::Engine(boa_cat::Error::Unsupported { feature: "test" })
}

#[test]
fn add_listener_then_dispatch_fires_the_handler() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let count = 0;
        const host = document.getElementById('host');
        host.addEventListener('click', () => { count = count + 1; });
        host.dispatchEvent({ type: 'click' });
        count",
    )?;
    matches!(value, Value::Number(n) if (n - 1.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected handler to fire once on dispatch"))
}

#[test]
fn dispatch_event_returns_true() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.dispatchEvent({ type: 'noop' })",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected dispatchEvent to return true"))
}

#[test]
fn multiple_listeners_fire_in_registration_order() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let trace = '';
        const host = document.getElementById('host');
        host.addEventListener('click', () => { trace = trace + 'a'; });
        host.addEventListener('click', () => { trace = trace + 'b'; });
        host.addEventListener('click', () => { trace = trace + 'c'; });
        host.dispatchEvent({ type: 'click' });
        trace",
    )?;
    matches!(value, Value::String(ref s) if s == "abc")
        .then_some(())
        .ok_or_else(|| fail("expected listeners to fire in registration order"))
}

#[test]
fn listeners_for_different_types_dont_cross_fire() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let clicks = 0;
        let submits = 0;
        const host = document.getElementById('host');
        host.addEventListener('click', () => { clicks = clicks + 1; });
        host.addEventListener('submit', () => { submits = submits + 1; });
        host.dispatchEvent({ type: 'click' });
        host.dispatchEvent({ type: 'click' });
        host.dispatchEvent({ type: 'submit' });
        clicks * 10 + submits",
    )?;
    matches!(value, Value::Number(n) if (n - 21.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected clicks=2 submits=1 (encoded as 21)"))
}

#[test]
fn handler_receives_the_event_object_with_type() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let received = '';
        const host = document.getElementById('host');
        host.addEventListener('foo', (e) => { received = e.type; });
        host.dispatchEvent({ type: 'foo' });
        received",
    )?;
    matches!(value, Value::String(ref s) if s == "foo")
        .then_some(())
        .ok_or_else(|| fail("expected handler to receive event with .type"))
}

#[test]
fn remove_event_listener_drops_the_handler() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let count = 0;
        const host = document.getElementById('host');
        const handler = () => { count = count + 1; };
        host.addEventListener('click', handler);
        host.dispatchEvent({ type: 'click' });
        host.removeEventListener('click', handler);
        host.dispatchEvent({ type: 'click' });
        count",
    )?;
    matches!(value, Value::Number(n) if (n - 1.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected count=1 after add/dispatch/remove/dispatch"))
}

#[test]
fn bubble_dispatch_fires_parent_listener() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='parent'><span id='child'>x</span></div></body></html>",
        "let trace = '';
        const child = document.getElementById('child');
        const parent = document.getElementById('parent');
        child.addEventListener('click', () => { trace = trace + 'c'; });
        parent.addEventListener('click', () => { trace = trace + 'p'; });
        child.dispatchEvent({ type: 'click' });
        trace",
    )?;
    matches!(value, Value::String(ref s) if s == "cp")
        .then_some(())
        .ok_or_else(|| fail("expected bubble: child then parent"))
}

#[test]
fn dispatch_on_root_doesnt_bubble_past_document_element() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let count = 0;
        const host = document.getElementById('host');
        const root = document.documentElement;
        root.addEventListener('click', () => { count = count + 1; });
        host.dispatchEvent({ type: 'click' });
        count",
    )?;
    matches!(value, Value::Number(n) if (n - 1.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected root listener to fire once via bubble"))
}

#[test]
fn listener_throw_does_not_abort_remaining_listeners() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let trace = '';
        const host = document.getElementById('host');
        host.addEventListener('click', () => { trace = trace + 'a'; throw 'boom'; });
        host.addEventListener('click', () => { trace = trace + 'b'; });
        try { host.dispatchEvent({ type: 'click' }); } catch (e) {}
        trace",
    )?;
    matches!(value, Value::String(ref s) if s == "ab")
        .then_some(())
        .ok_or_else(|| fail("expected second listener to still fire after first throws"))
}

#[test]
fn add_event_listener_works_on_created_element() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "let count = 0;
        const el = document.createElement('span');
        el.addEventListener('ping', () => { count = count + 1; });
        el.dispatchEvent({ type: 'ping' });
        count",
    )?;
    matches!(value, Value::Number(n) if (n - 1.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected event system to work on createElement output"))
}

#[test]
fn handler_can_close_over_outer_state() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let counter = { n: 0 };
        const host = document.getElementById('host');
        host.addEventListener('tick', () => { counter.n = counter.n + 1; });
        host.dispatchEvent({ type: 'tick' });
        host.dispatchEvent({ type: 'tick' });
        host.dispatchEvent({ type: 'tick' });
        counter.n",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected handler closure to accumulate state across 3 dispatches"))
}

#[test]
fn dispatch_with_no_listeners_is_a_silent_noop() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.dispatchEvent({ type: 'unhandled' })",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected silent no-op when no listeners are registered"))
}

#[test]
fn prevent_default_makes_dispatch_event_return_false() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.addEventListener('click', (e) => { e.preventDefault(); });
        host.dispatchEvent({ type: 'click' })",
    )?;
    matches!(value, Value::Boolean(false))
        .then_some(())
        .ok_or_else(|| fail("expected dispatchEvent to return false after preventDefault"))
}

#[test]
fn prevent_default_sets_default_prevented_flag_on_event() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let observed = 'unset';
        const host = document.getElementById('host');
        host.addEventListener('click', (e) => { e.preventDefault(); });
        host.addEventListener('click', (e) => { observed = e.defaultPrevented; });
        host.dispatchEvent({ type: 'click' });
        observed",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected defaultPrevented flag visible to subsequent listeners"))
}

#[test]
fn dispatch_without_prevent_default_returns_true() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.addEventListener('click', () => {});
        host.dispatchEvent({ type: 'click' })",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| {
            fail("expected dispatchEvent to return true when no preventDefault was called")
        })
}

#[test]
fn stop_propagation_halts_bubble_after_current_level() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='parent'><span id='child'>x</span></div></body></html>",
        "let trace = '';
        const child = document.getElementById('child');
        const parent = document.getElementById('parent');
        child.addEventListener('click', (e) => { trace = trace + 'c'; e.stopPropagation(); });
        parent.addEventListener('click', () => { trace = trace + 'p'; });
        child.dispatchEvent({ type: 'click' });
        trace",
    )?;
    matches!(value, Value::String(ref s) if s == "c")
        .then_some(())
        .ok_or_else(|| fail("expected stopPropagation to skip the parent listener"))
}

#[test]
fn stop_propagation_still_fires_remaining_listeners_at_current_level() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='parent'><span id='child'>x</span></div></body></html>",
        "let trace = '';
        const child = document.getElementById('child');
        const parent = document.getElementById('parent');
        child.addEventListener('click', (e) => { trace = trace + 'a'; e.stopPropagation(); });
        child.addEventListener('click', () => { trace = trace + 'b'; });
        parent.addEventListener('click', () => { trace = trace + 'p'; });
        child.dispatchEvent({ type: 'click' });
        trace",
    )?;
    matches!(value, Value::String(ref s) if s == "ab")
        .then_some(())
        .ok_or_else(|| fail("expected stopPropagation to keep sibling listeners but skip parent"))
}

#[test]
fn stop_immediate_propagation_halts_remaining_listeners_at_current_level() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='parent'><span id='child'>x</span></div></body></html>",
        "let trace = '';
        const child = document.getElementById('child');
        const parent = document.getElementById('parent');
        child.addEventListener('click', (e) => { trace = trace + 'a'; e.stopImmediatePropagation(); });
        child.addEventListener('click', () => { trace = trace + 'b'; });
        parent.addEventListener('click', () => { trace = trace + 'p'; });
        child.dispatchEvent({ type: 'click' });
        trace",
    )?;
    matches!(value, Value::String(ref s) if s == "a")
        .then_some(())
        .ok_or_else(|| fail("expected stopImmediatePropagation to skip BOTH sibling and parent"))
}

#[test]
fn event_target_stays_as_original_dispatch_target() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='parent'><span id='child'>x</span></div></body></html>",
        "let observed = '';
        const child = document.getElementById('child');
        const parent = document.getElementById('parent');
        parent.addEventListener('click', (e) => { observed = e.target.tagName; });
        child.dispatchEvent({ type: 'click' });
        observed",
    )?;
    matches!(value, Value::String(ref s) if s == "span")
        .then_some(())
        .ok_or_else(|| fail("expected event.target to be the original dispatch target (span)"))
}

#[test]
fn event_current_target_reflects_current_bubble_level() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='parent'><span id='child'>x</span></div></body></html>",
        "let trace = '';
        const child = document.getElementById('child');
        const parent = document.getElementById('parent');
        child.addEventListener('click', (e) => { trace = trace + e.currentTarget.tagName; });
        parent.addEventListener('click', (e) => { trace = trace + ',' + e.currentTarget.tagName; });
        child.dispatchEvent({ type: 'click' });
        trace",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("span,div"))
        .then_some(())
        .ok_or_else(|| fail("expected currentTarget to update per bubble level"))
}

#[test]
fn event_type_is_preserved_through_decoration() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "let received = '';
        const host = document.getElementById('host');
        host.addEventListener('myevent', (e) => { received = e.type; });
        host.dispatchEvent({ type: 'myevent' });
        received",
    )?;
    matches!(value, Value::String(ref s) if s == "myevent")
        .then_some(())
        .ok_or_else(|| fail("expected event.type to survive dispatch decoration"))
}

#[test]
fn prevent_default_does_not_affect_bubble() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='parent'><span id='child'>x</span></div></body></html>",
        "let trace = '';
        const child = document.getElementById('child');
        const parent = document.getElementById('parent');
        child.addEventListener('click', (e) => { e.preventDefault(); trace = trace + 'c'; });
        parent.addEventListener('click', () => { trace = trace + 'p'; });
        child.dispatchEvent({ type: 'click' });
        trace",
    )?;
    matches!(value, Value::String(ref s) if s == "cp")
        .then_some(())
        .ok_or_else(|| fail("expected bubble to still fire parent listener after preventDefault"))
}

#[test]
fn prevent_default_is_idempotent() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        host.addEventListener('click', (e) => {
            e.preventDefault();
            e.preventDefault();
            e.preventDefault();
        });
        host.dispatchEvent({ type: 'click' })",
    )?;
    matches!(value, Value::Boolean(false))
        .then_some(())
        .ok_or_else(|| fail("expected three preventDefault calls to still return false"))
}

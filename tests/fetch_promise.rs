//! `fetch(url)` (v0.7.1): returns a `Promise<Response>` rather than
//! a Response directly.  Tests cover the rejected-Promise path
//! since the live success path requires a network mock that this
//! crate doesn't ship.

use boa_cat::env::Env;
use boa_cat::evaluate_program_with;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::promise::PromiseState;
use boa_cat::value::Value;
use ecma_lex_cat::lex;
use ecma_parse_cat::parse_script;
use web_api_cat::Error;

fn run(html: &str, script: &str) -> Result<Value, Error> {
    let html_doc = html_cat::parse(html)?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let tokens = lex(script).map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(500_000)).map_err(Error::from)?;
    Ok(drain_promise(value, &heap))
}

fn drain_promise(value: Value, heap: &Heap) -> Value {
    match value {
        Value::Promise(id) => heap
            .promise(id)
            .and_then(|state| match state {
                PromiseState::Resolved(v) | PromiseState::Rejected(v) => Some(v.clone()),
                PromiseState::Pending(_) => None,
            })
            .unwrap_or(Value::Undefined),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Object(_)
        | Value::Function(_)
        | Value::Native(_) => value,
    }
}

fn fail(_msg: &'static str) -> Error {
    Error::Engine(boa_cat::Error::Unsupported { feature: "test" })
}

#[test]
fn fetch_no_args_returns_a_promise() -> Result<(), Error> {
    let value = run("<html><body></body></html>", "typeof fetch()")?;
    matches!(value, Value::String(ref s) if s == "object")
        .then_some(())
        .ok_or_else(|| fail("expected typeof fetch() === 'object' (Promise)"))
}

#[test]
fn fetch_bad_url_returns_a_promise() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "typeof fetch('not-a-real-url').then",
    )?;
    matches!(value, Value::String(ref s) if s == "function")
        .then_some(())
        .ok_or_else(|| fail("expected fetch(...).then to be a function (Promise shape)"))
}

#[test]
fn fetch_no_args_promise_is_rejected() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "(async () => {
            try {
                await fetch();
                return 'no-throw';
            } catch (e) {
                return 'caught';
            }
        })()",
    )?;
    matches!(value, Value::String(ref s) if s == "caught")
        .then_some(())
        .ok_or_else(|| fail("expected fetch() with no args to reject and be caught"))
}

#[test]
fn fetch_bad_url_promise_is_rejected() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "(async () => {
            try {
                await fetch('not-a-real-url');
                return 'no-throw';
            } catch (e) {
                return 'caught';
            }
        })()",
    )?;
    matches!(value, Value::String(ref s) if s == "caught")
        .then_some(())
        .ok_or_else(|| fail("expected fetch with bad URL to reject and be caught"))
}

#[test]
fn fetch_rejected_promise_dispatches_catch() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "(async () => {
            let outcome = 'pending';
            await fetch('not-a-real-url').catch(_ => { outcome = 'caught-via-catch'; });
            return outcome;
        })()",
    )?;
    matches!(value, Value::String(ref s) if s == "caught-via-catch")
        .then_some(())
        .ok_or_else(|| fail("expected .catch(handler) on a rejected fetch promise to fire"))
}

#[test]
fn fetch_rejection_message_surfaces_in_catch() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "(async () => {
            try {
                await fetch();
            } catch (e) {
                return typeof e === 'string' ? e : 'wrong-shape';
            }
        })()",
    )?;
    matches!(value, Value::String(ref s) if s == "TypeError: fetch() requires a URL string")
        .then_some(())
        .ok_or_else(|| fail("expected rejection reason to be the TypeError string"))
}

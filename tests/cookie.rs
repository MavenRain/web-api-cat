//! Tests for the `document.cookie` host/JS bridge.

use boa_cat::env::Binding;
use boa_cat::env::Env;
use boa_cat::evaluate_program_with;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::value::Value;
use ecma_lex_cat::lex;
use ecma_parse_cat::parse_script;
use web_api_cat::Error;
use web_api_cat::{get_document_cookie, set_document_cookie};

fn fail(_msg: &'static str) -> Error {
    Error::Engine(boa_cat::Error::Unsupported { feature: "test" })
}

fn document_value(env: &Env, heap: &Heap) -> Option<Value> {
    env.lookup("document").and_then(|binding| match binding {
        Binding::Direct(value) => Some(value.clone()),
        Binding::Cell(cell_id) => heap.cell(*cell_id).map(|cell| cell.value().clone()),
    })
}

#[test]
fn default_cookie_is_empty_string() -> Result<(), Error> {
    let html = html_cat::parse("<html><body></body></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html);
    let document = document_value(&env, &heap).ok_or_else(|| fail("document missing"))?;
    let snapshot = get_document_cookie(&document, &heap);
    snapshot
        .as_deref()
        .filter(|s| s.is_empty())
        .map(|_| ())
        .ok_or_else(|| fail("expected empty initial cookie"))
}

#[test]
fn set_document_cookie_then_read_back() -> Result<(), Error> {
    let html = html_cat::parse("<html><body></body></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html);
    let document = document_value(&env, &heap).ok_or_else(|| fail("document missing"))?;
    let heap = set_document_cookie(&document, heap, "session=abc; theme=dark");
    let snapshot = get_document_cookie(&document, &heap);
    snapshot
        .as_deref()
        .filter(|s| *s == "session=abc; theme=dark")
        .map(|_| ())
        .ok_or_else(|| fail("expected serialized jar"))
}

#[test]
fn js_can_read_document_cookie() -> Result<(), Error> {
    let html = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html);
    let document = document_value(&env, &heap).ok_or_else(|| fail("document missing"))?;
    let heap = set_document_cookie(&document, heap, "session=abc");
    let tokens = lex("document.cookie").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, _heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(100_000)).map_err(Error::from)?;
    matches!(value, Value::String(ref s) if s == "session=abc")
        .then_some(())
        .ok_or_else(|| fail("expected the cookie string from JS"))
}

#[test]
fn js_overwrite_visible_to_host_via_get_document_cookie() -> Result<(), Error> {
    let html = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html);
    let document = document_value(&env, &heap).ok_or_else(|| fail("document missing"))?;
    let heap = set_document_cookie(&document, heap, "old=1");
    let tokens = lex("document.cookie = 'new=2'; document.cookie").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(100_000)).map_err(Error::from)?;
    let js_view = matches!(&value, Value::String(s) if s == "new=2");
    let host_view = get_document_cookie(&document, &heap)
        .as_deref()
        .is_some_and(|s| s == "new=2");
    (js_view && host_view)
        .then_some(())
        .ok_or_else(|| fail("JS write should be visible to host"))
}

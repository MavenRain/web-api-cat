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
fn js_write_merges_into_visible_jar() -> Result<(), Error> {
    // v0.4: writes go through an accessor's setter that merges by
    // name, mirroring real browser semantics.  Existing "old=1"
    // stays; "new=2" joins it; both are visible to JS and the host.
    let html = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html);
    let document = document_value(&env, &heap).ok_or_else(|| fail("document missing"))?;
    let heap = set_document_cookie(&document, heap, "old=1");
    let tokens = lex("document.cookie = 'new=2'; document.cookie").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(100_000)).map_err(Error::from)?;
    let js_view = matches!(&value, Value::String(s) if s == "old=1; new=2");
    let host_view = web_api_cat::get_document_cookie(&document, &heap)
        .as_deref()
        .is_some_and(|s| s == "old=1; new=2");
    (js_view && host_view)
        .then_some(())
        .ok_or_else(|| fail("JS write should merge by name into the visible jar"))
}

#[test]
fn js_write_with_same_name_replaces_by_name() -> Result<(), Error> {
    let html = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html);
    let document = document_value(&env, &heap).ok_or_else(|| fail("document missing"))?;
    let heap = set_document_cookie(&document, heap, "session=old");
    let tokens =
        lex("document.cookie = 'session=new'; document.cookie").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, _heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(100_000)).map_err(Error::from)?;
    matches!(&value, Value::String(s) if s == "session=new")
        .then_some(())
        .ok_or_else(|| fail("same-name write should replace the existing entry"))
}

#[test]
fn js_write_with_attributes_logged_for_host() -> Result<(), Error> {
    // The attributes don't appear in the visible projection
    // (browsers strip them from document.cookie reads), but they
    // DO surface in read_cookie_writes so the host can parse each
    // entry and merge attribute-bearing cookies into its real jar.
    let html = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html);
    let document = document_value(&env, &heap).ok_or_else(|| fail("document missing"))?;
    let heap = set_document_cookie(&document, heap, "");
    let tokens = lex("document.cookie = 'session=abc; Max-Age=600; Path=/admin'")
        .map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (_value, heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(100_000)).map_err(Error::from)?;
    let writes = web_api_cat::read_cookie_writes(&document, &heap);
    let first = writes.first().ok_or_else(|| fail("expected one write"))?;
    let visible = web_api_cat::get_document_cookie(&document, &heap).unwrap_or_default();
    (writes.len() == 1
        && first == "session=abc; Max-Age=600; Path=/admin"
        && visible == "session=abc")
        .then_some(())
        .ok_or_else(|| fail("write log should preserve attributes; visible should strip them"))
}

#[test]
fn multiple_writes_log_in_order() -> Result<(), Error> {
    let html = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html);
    let document = document_value(&env, &heap).ok_or_else(|| fail("document missing"))?;
    let heap = set_document_cookie(&document, heap, "");
    let tokens = lex(
        "document.cookie = 'a=1; Path=/'; document.cookie = 'b=2; Max-Age=600'; document.cookie",
    )
    .map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(100_000)).map_err(Error::from)?;
    let writes = web_api_cat::read_cookie_writes(&document, &heap);
    let visible_via_js = matches!(&value, Value::String(s) if s == "a=1; b=2");
    (writes == vec!["a=1; Path=/".to_owned(), "b=2; Max-Age=600".to_owned()] && visible_via_js)
        .then_some(())
        .ok_or_else(|| fail("expected ordered write log + merged visible string"))
}

#[test]
fn pre_eval_set_clears_previous_write_log() -> Result<(), Error> {
    // A second host seed (e.g. before the next eval) should drop
    // the log so the post-eval read of that next run doesn't pick
    // up writes from the prior one.
    let html = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html);
    let document = document_value(&env, &heap).ok_or_else(|| fail("document missing"))?;
    let heap = set_document_cookie(&document, heap, "");
    let tokens = lex("document.cookie = 'a=1'").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (_value, heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(100_000)).map_err(Error::from)?;
    let heap = set_document_cookie(&document, heap, "fresh=2");
    let writes = web_api_cat::read_cookie_writes(&document, &heap);
    writes
        .is_empty()
        .then_some(())
        .ok_or_else(|| fail("set_document_cookie should reset the write log for the next eval"))
}

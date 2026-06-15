//! v0.7.22 Location API: `window.location` with `href`,
//! parsed-URL accessors, and navigation methods that record
//! intents downstream consumers can act on.

use boa_cat::env::Env;
use boa_cat::evaluate_program_with;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::value::Value;
use ecma_lex_cat::lex;
use ecma_parse_cat::parse_script;
use web_api_cat::Error;
use web_api_cat::location::{
    NavigationIntent, clear_pending_navigation, read_pending_navigation, set_location_href,
};

fn run(html: &str, script: &str) -> Result<Value, Error> {
    let html_doc = html_cat::parse(html)?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let tokens = lex(script).map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, _heap) =
        evaluate_program_with(&program, env, heap, Fuel::new(200_000)).map_err(Error::from)?;
    Ok(value)
}

fn run_with_href(html: &str, href: &str, script: &str) -> Result<Value, Error> {
    let html_doc = html_cat::parse(html)?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let heap = set_location_href(&env, href, heap);
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
fn location_default_href_is_empty() -> Result<(), Error> {
    let value = run("<html></html>", "location.href")?;
    matches!(value, Value::String(ref s) if s.is_empty())
        .then_some(())
        .ok_or_else(|| fail("expected default href to be ''"))
}

#[test]
fn location_href_reflects_seeded_url() -> Result<(), Error> {
    let value = run_with_href(
        "<html></html>",
        "https://example.com/path?q=1#frag",
        "location.href",
    )?;
    matches!(value, Value::String(ref s) if s == "https://example.com/path?q=1#frag")
        .then_some(())
        .ok_or_else(|| fail("expected location.href to mirror set_location_href"))
}

#[test]
fn location_protocol_host_pathname_search_hash() -> Result<(), Error> {
    let value = run_with_href(
        "<html></html>",
        "https://example.com:8080/about?lang=en#top",
        "location.protocol + '|' + location.host + '|' + location.hostname + '|'
         + location.port + '|' + location.pathname + '|' + location.search + '|'
         + location.hash + '|' + location.origin",
    )?;
    matches!(value, Value::String(ref s) if
        s == "https:|example.com:8080|example.com|8080|/about|?lang=en|#top|https://example.com:8080"
    )
        .then_some(())
        .ok_or_else(|| fail("expected parsed URL fields to match"))
}

#[test]
fn location_href_setter_records_assign_intent() -> Result<(), Error> {
    let html_doc = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let tokens = lex("location.href = '/next'").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (_value, heap) = evaluate_program_with(&program, env.clone(), heap, Fuel::new(50_000))
        .map_err(Error::from)?;
    let intent = read_pending_navigation(&env, &heap).ok_or_else(|| fail("no intent recorded"))?;
    matches!(intent, NavigationIntent::Assign(ref url) if url == "/next")
        .then_some(())
        .ok_or_else(|| fail("expected Assign('/next') intent"))
}

#[test]
fn location_assign_records_assign_intent() -> Result<(), Error> {
    let html_doc = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let tokens = lex("location.assign('/x')").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (_value, heap) = evaluate_program_with(&program, env.clone(), heap, Fuel::new(50_000))
        .map_err(Error::from)?;
    let intent = read_pending_navigation(&env, &heap).ok_or_else(|| fail("no intent recorded"))?;
    matches!(intent, NavigationIntent::Assign(ref url) if url == "/x")
        .then_some(())
        .ok_or_else(|| fail("expected Assign('/x') intent"))
}

#[test]
fn location_replace_records_replace_intent() -> Result<(), Error> {
    let html_doc = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let tokens = lex("location.replace('/y')").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (_value, heap) = evaluate_program_with(&program, env.clone(), heap, Fuel::new(50_000))
        .map_err(Error::from)?;
    let intent = read_pending_navigation(&env, &heap).ok_or_else(|| fail("no intent recorded"))?;
    matches!(intent, NavigationIntent::Replace(ref url) if url == "/y")
        .then_some(())
        .ok_or_else(|| fail("expected Replace('/y') intent"))
}

#[test]
fn location_reload_records_reload_intent() -> Result<(), Error> {
    let html_doc = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let tokens = lex("location.reload()").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (_value, heap) = evaluate_program_with(&program, env.clone(), heap, Fuel::new(50_000))
        .map_err(Error::from)?;
    let intent = read_pending_navigation(&env, &heap).ok_or_else(|| fail("no intent recorded"))?;
    matches!(intent, NavigationIntent::Reload)
        .then_some(())
        .ok_or_else(|| fail("expected Reload intent"))
}

#[test]
fn no_pending_nav_when_script_does_not_navigate() -> Result<(), Error> {
    let html_doc = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let tokens = lex("location.href").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (_value, heap) = evaluate_program_with(&program, env.clone(), heap, Fuel::new(50_000))
        .map_err(Error::from)?;
    read_pending_navigation(&env, &heap)
        .is_none()
        .then_some(())
        .ok_or_else(|| fail("expected no pending nav from a pure read"))
}

#[test]
fn clear_pending_navigation_resets_intent() -> Result<(), Error> {
    let html_doc = html_cat::parse("<html></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let tokens = lex("location.assign('/x')").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (_value, heap) = evaluate_program_with(&program, env.clone(), heap, Fuel::new(50_000))
        .map_err(Error::from)?;
    let heap = clear_pending_navigation(&env, heap);
    read_pending_navigation(&env, &heap)
        .is_none()
        .then_some(())
        .ok_or_else(|| fail("expected cleared intent to read None"))
}

#[test]
fn window_location_is_same_object_as_global_location() -> Result<(), Error> {
    let value = run(
        "<html></html>",
        "window.location === location ? 'same' : 'different'",
    )?;
    matches!(value, Value::String(ref s) if s == "same")
        .then_some(())
        .ok_or_else(|| fail("expected window.location to be the same object as location"))
}

#[test]
fn location_to_string_returns_href() -> Result<(), Error> {
    let value = run_with_href(
        "<html></html>",
        "https://example.com/",
        "location.toString()",
    )?;
    matches!(value, Value::String(ref s) if s == "https://example.com/")
        .then_some(())
        .ok_or_else(|| fail("expected location.toString() to return href"))
}

#[test]
fn parsed_fields_are_empty_for_invalid_url() -> Result<(), Error> {
    let value = run_with_href(
        "<html></html>",
        "not a valid url",
        "location.protocol + '|' + location.host + '|' + location.pathname",
    )?;
    matches!(value, Value::String(ref s) if s == "||")
        .then_some(())
        .ok_or_else(|| fail("expected empty parsed fields for invalid URL"))
}

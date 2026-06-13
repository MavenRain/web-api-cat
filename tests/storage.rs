//! `localStorage` / `sessionStorage` (v0.7.0): Storage-shaped
//! Objects with getItem / setItem / removeItem / clear / key /
//! length, plus host-side seeding and post-eval read helpers.

use boa_cat::env::Env;
use boa_cat::evaluate_program_with;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::value::Value;
use ecma_lex_cat::lex;
use ecma_parse_cat::parse_script;
use web_api_cat::Error;
use web_api_cat::storage::{
    lookup_local_storage, lookup_session_storage, read_storage_items, seed_storage,
};

fn install_with_seed(
    html: &str,
    seed: &[(String, String)],
) -> Result<(boa_cat::env::Env, boa_cat::heap::Heap), Error> {
    let html_doc = html_cat::parse(html)?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);
    let storage = lookup_local_storage(&env, &heap).ok_or_else(no_storage)?;
    let heap = seed_storage(&storage, heap, seed);
    Ok((env, heap))
}

fn no_storage() -> Error {
    Error::Engine(boa_cat::Error::Unsupported {
        feature: "localStorage missing from env",
    })
}

fn run(
    html: &str,
    seed: &[(String, String)],
    script: &str,
) -> Result<(Value, boa_cat::heap::Heap, boa_cat::env::Env), Error> {
    let (env, heap) = install_with_seed(html, seed)?;
    let tokens = lex(script).map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, heap) = evaluate_program_with(&program, env.clone(), heap, Fuel::new(200_000))
        .map_err(Error::from)?;
    Ok((value, heap, env))
}

fn fail(_msg: &'static str) -> Error {
    Error::Engine(boa_cat::Error::Unsupported { feature: "test" })
}

#[test]
fn local_storage_set_then_get_round_trips() -> Result<(), Error> {
    let (value, _, _) = run(
        "<html><body></body></html>",
        &[],
        "localStorage.setItem('k', 'v');
        localStorage.getItem('k')",
    )?;
    matches!(value, Value::String(ref s) if s == "v")
        .then_some(())
        .ok_or_else(|| fail("expected getItem to return what setItem stored"))
}

#[test]
fn local_storage_get_missing_returns_null() -> Result<(), Error> {
    let (value, _, _) = run(
        "<html><body></body></html>",
        &[],
        "localStorage.getItem('nope') === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected getItem on missing key to return null"))
}

#[test]
fn local_storage_seeded_items_are_visible() -> Result<(), Error> {
    let seed = vec![
        ("alpha".to_owned(), "1".to_owned()),
        ("beta".to_owned(), "2".to_owned()),
    ];
    let (value, _, _) = run(
        "<html><body></body></html>",
        &seed,
        "localStorage.getItem('alpha') + ',' + localStorage.getItem('beta')",
    )?;
    matches!(value, Value::String(ref s) if s == "1,2")
        .then_some(())
        .ok_or_else(|| fail("expected seeded items to be readable"))
}

#[test]
fn local_storage_length_reflects_item_count() -> Result<(), Error> {
    let (value, _, _) = run(
        "<html><body></body></html>",
        &[],
        "localStorage.setItem('a', '1');
        localStorage.setItem('b', '2');
        localStorage.setItem('c', '3');
        localStorage.length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected length === 3 after three setItem calls"))
}

#[test]
fn local_storage_remove_item_drops_entry() -> Result<(), Error> {
    let (value, _, _) = run(
        "<html><body></body></html>",
        &[("k".to_owned(), "v".to_owned())],
        "localStorage.removeItem('k');
        localStorage.getItem('k') === null ? 'gone' : 'still-here'",
    )?;
    matches!(value, Value::String(ref s) if s == "gone")
        .then_some(())
        .ok_or_else(|| fail("expected removeItem to drop the entry"))
}

#[test]
fn local_storage_clear_empties_storage() -> Result<(), Error> {
    let (value, _, _) = run(
        "<html><body></body></html>",
        &[
            ("a".to_owned(), "1".to_owned()),
            ("b".to_owned(), "2".to_owned()),
        ],
        "localStorage.clear();
        localStorage.length",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected clear() to drop length to 0"))
}

#[test]
fn local_storage_key_returns_nth_key_in_btree_order() -> Result<(), Error> {
    let (value, _, _) = run(
        "<html><body></body></html>",
        &[],
        "localStorage.setItem('zebra', '1');
        localStorage.setItem('alpha', '1');
        localStorage.setItem('mango', '1');
        localStorage.key(0) + ',' + localStorage.key(1) + ',' + localStorage.key(2)",
    )?;
    matches!(value, Value::String(ref s) if s == "alpha,mango,zebra")
        .then_some(())
        .ok_or_else(|| fail("expected key(i) to return BTreeMap-sorted keys"))
}

#[test]
fn local_storage_key_out_of_range_returns_null() -> Result<(), Error> {
    let (value, _, _) = run(
        "<html><body></body></html>",
        &[("k".to_owned(), "v".to_owned())],
        "localStorage.key(5) === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected key(out-of-range) to be null"))
}

#[test]
fn host_reads_post_eval_items() -> Result<(), Error> {
    let (_, heap, env) = run(
        "<html><body></body></html>",
        &[],
        "localStorage.setItem('alpha', 'one');
        localStorage.setItem('beta', 'two');",
    )?;
    let storage = lookup_local_storage(&env, &heap).ok_or_else(no_storage)?;
    let items = read_storage_items(&storage, &heap);
    (items
        == vec![
            ("alpha".to_owned(), "one".to_owned()),
            ("beta".to_owned(), "two".to_owned()),
        ])
    .then_some(())
    .ok_or_else(|| fail("expected post-eval read to surface setItem writes"))
}

#[test]
fn session_storage_is_independent_of_local() -> Result<(), Error> {
    let (value, _, _) = run(
        "<html><body></body></html>",
        &[],
        "localStorage.setItem('shared', 'local');
        sessionStorage.setItem('shared', 'session');
        localStorage.getItem('shared') + ',' + sessionStorage.getItem('shared')",
    )?;
    matches!(value, Value::String(ref s) if s == "local,session")
        .then_some(())
        .ok_or_else(|| fail("expected local and session storage to be independent"))
}

#[test]
fn window_local_storage_is_the_same_object_as_global_local_storage() -> Result<(), Error> {
    let (value, _, _) = run(
        "<html><body></body></html>",
        &[],
        "localStorage.setItem('k', 'v');
        window.localStorage.getItem('k')",
    )?;
    matches!(value, Value::String(ref s) if s == "v")
        .then_some(())
        .ok_or_else(|| fail("expected window.localStorage === localStorage"))
}

#[test]
fn host_seed_then_post_eval_read_round_trips_unchanged_state() -> Result<(), Error> {
    let seed = vec![("kept".to_owned(), "as-is".to_owned())];
    let (_, heap, env) = run(
        "<html><body></body></html>",
        &seed,
        "localStorage.getItem('kept')",
    )?;
    let storage = lookup_local_storage(&env, &heap).ok_or_else(no_storage)?;
    let items = read_storage_items(&storage, &heap);
    (items == seed)
        .then_some(())
        .ok_or_else(|| fail("expected seed to survive a read-only script"))
}

#[test]
fn session_storage_lookup_works() -> Result<(), Error> {
    let (_, heap, env) = run(
        "<html><body></body></html>",
        &[],
        "sessionStorage.setItem('s', 'val');",
    )?;
    let session = lookup_session_storage(&env, &heap).ok_or_else(no_storage)?;
    let items = read_storage_items(&session, &heap);
    (items == vec![("s".to_owned(), "val".to_owned())])
        .then_some(())
        .ok_or_else(|| fail("expected lookup_session_storage + read to round-trip"))
}

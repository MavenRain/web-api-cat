//! `Element.querySelectorAll(selector)` and
//! `document.querySelectorAll(selector)` (v0.7.11): companion to
//! v0.7.8's `querySelector`, returns a NodeList-shaped Object of
//! all matching descendants instead of stopping at the first.

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
fn query_selector_all_returns_node_list_with_length() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p id='a'>x</p>
            <p id='b'>y</p>
            <p id='c'>z</p>
        </body></html>",
        "document.querySelectorAll('p').length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected querySelectorAll('p').length === 3"))
}

#[test]
fn query_selector_all_entries_indexed_in_document_order() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p id='one'>x</p>
            <p id='two'>y</p>
            <p id='three'>z</p>
        </body></html>",
        "const ps = document.querySelectorAll('p');
        ps[0].id + ',' + ps[1].id + ',' + ps[2].id",
    )?;
    matches!(value, Value::String(ref s) if s == "one,two,three")
        .then_some(())
        .ok_or_else(|| fail("expected entries to land at numeric keys in document order"))
}

#[test]
fn query_selector_all_no_match_yields_empty_node_list() -> Result<(), Error> {
    let value = run(
        "<html><body><div></div></body></html>",
        "document.querySelectorAll('.never').length",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected no-match to yield length === 0"))
}

#[test]
fn query_selector_all_respects_class_filter() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p class='want'>x</p>
            <p class='skip'>y</p>
            <p class='want'>z</p>
            <span class='want'>q</span>
        </body></html>",
        "document.querySelectorAll('p.want').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected only 2 p.want elements (the span shouldn't count)"))
}

#[test]
fn query_selector_all_supports_descendant_combinator() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <section>
                <article><p>nested</p></article>
                <p>direct</p>
            </section>
            <section>
                <p>second-section</p>
            </section>
        </body></html>",
        "document.querySelectorAll('section p').length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected 3 p elements via 'section p' descendant combinator"))
}

#[test]
fn query_selector_all_supports_selector_list() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <h1>a</h1>
            <h2>b</h2>
            <h3>c</h3>
            <p>d</p>
        </body></html>",
        "document.querySelectorAll('h1, h3').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected selector list 'h1, h3' to match both"))
}

#[test]
fn element_query_selector_all_scopes_to_subtree() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <div id='scope'>
                <p>scoped</p>
                <p>scoped2</p>
            </div>
            <p>outside</p>
        </body></html>",
        "document.getElementById('scope').querySelectorAll('p').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| {
            fail("expected element.querySelectorAll to skip elements outside the subtree")
        })
}

#[test]
fn query_selector_all_iteration_via_length_index_loop() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p>a</p>
            <p>b</p>
            <p>c</p>
        </body></html>",
        "const ps = document.querySelectorAll('p');
        let acc = '';
        for (let i = 0; i < ps.length; i = i + 1) {
            acc = acc + ps[i].textContent;
        }
        acc",
    )?;
    matches!(value, Value::String(ref s) if s == "abc")
        .then_some(())
        .ok_or_else(|| {
            fail("expected for-loop over NodeList to visit each match's textContent in order")
        })
}

#[test]
fn query_selector_all_attribute_selector() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <a href='/a'>a</a>
            <a>plain</a>
            <a href='/b'>b</a>
        </body></html>",
        "document.querySelectorAll('a[href]').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected attribute selector to filter to anchors with href"))
}

#[test]
fn query_selector_all_on_document_walks_root() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p>x</p>
            <div><p>y</p></div>
        </body></html>",
        "document.querySelectorAll('p').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| {
            fail("expected document.querySelectorAll to walk both top-level and nested p")
        })
}

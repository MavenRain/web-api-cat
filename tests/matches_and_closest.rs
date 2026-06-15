//! `Element.matches(selector)` + `Element.closest(selector)` +
//! `:first-child` / `:last-child` / `:only-child` pseudo-classes
//! (v0.7.13).

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
fn matches_positive_tag() -> Result<(), Error> {
    let value = run(
        "<html><body><p id='host'>x</p></body></html>",
        "document.getElementById('host').matches('p')",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected matches('p') to be true on a p element"))
}

#[test]
fn matches_negative_tag() -> Result<(), Error> {
    let value = run(
        "<html><body><p id='host'>x</p></body></html>",
        "document.getElementById('host').matches('div')",
    )?;
    matches!(value, Value::Boolean(false))
        .then_some(())
        .ok_or_else(|| fail("expected matches('div') to be false on a p element"))
}

#[test]
fn matches_with_class_compound() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='alpha beta'>x</div></body></html>",
        "document.getElementById('host').matches('div.alpha.beta')",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| {
            fail("expected matches('div.alpha.beta') to be true on a div with both classes")
        })
}

#[test]
fn matches_with_attribute_selector() -> Result<(), Error> {
    let value = run(
        "<html><body><a id='host' href='/x'>x</a></body></html>",
        "document.getElementById('host').matches('a[href]')",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected matches('a[href]') to be true when href is present"))
}

#[test]
fn matches_with_selector_list() -> Result<(), Error> {
    let value = run(
        "<html><body><h2 id='host'>x</h2></body></html>",
        "document.getElementById('host').matches('h1, h2, h3')",
    )?;
    matches!(value, Value::Boolean(true))
        .then_some(())
        .ok_or_else(|| fail("expected matches('h1, h2, h3') to match h2"))
}

#[test]
fn closest_returns_self_when_self_matches() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host' class='target'>x</div></body></html>",
        "document.getElementById('host').closest('.target').id",
    )?;
    matches!(value, Value::String(ref s) if s == "host")
        .then_some(())
        .ok_or_else(|| fail("expected closest to return self when self matches"))
}

#[test]
fn closest_walks_up_to_ancestor() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <section id='outer' class='target'>
                <article>
                    <p id='deep'>x</p>
                </article>
            </section>
        </body></html>",
        "document.getElementById('deep').closest('.target').id",
    )?;
    matches!(value, Value::String(ref s) if s == "outer")
        .then_some(())
        .ok_or_else(|| fail("expected closest to walk up to the matching ancestor"))
}

#[test]
fn closest_returns_null_when_no_match() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'>x</div></body></html>",
        "document.getElementById('host').closest('.never') === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected closest to return null when no ancestor matches"))
}

#[test]
fn closest_picks_nearest_when_multiple_ancestors_match() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <section class='wrap' id='outer'>
                <section class='wrap' id='inner'>
                    <p id='leaf'>x</p>
                </section>
            </section>
        </body></html>",
        "document.getElementById('leaf').closest('.wrap').id",
    )?;
    matches!(value, Value::String(ref s) if s == "inner")
        .then_some(())
        .ok_or_else(|| fail("expected closest to pick the nearest matching ancestor"))
}

#[test]
fn first_child_pseudo_class() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p>a</p><p>b</p><p>c</p></div></body></html>",
        "document.querySelector('p:first-child').textContent",
    )?;
    matches!(value, Value::String(ref s) if s == "a")
        .then_some(())
        .ok_or_else(|| fail("expected p:first-child to pick the first p"))
}

#[test]
fn last_child_pseudo_class() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><p>a</p><p>b</p><p>c</p></div></body></html>",
        "document.querySelector('p:last-child').textContent",
    )?;
    matches!(value, Value::String(ref s) if s == "c")
        .then_some(())
        .ok_or_else(|| fail("expected p:last-child to pick the last p"))
}

#[test]
fn only_child_pseudo_class() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <div id='solo-host'><p>solo</p></div>
            <div id='multi-host'><p>first</p><p>second</p></div>
        </body></html>",
        "document.querySelector('p:only-child').textContent",
    )?;
    matches!(value, Value::String(ref s) if s == "solo")
        .then_some(())
        .ok_or_else(|| fail("expected :only-child to pick the lone p"))
}

#[test]
fn pseudo_class_combines_with_class() -> Result<(), Error> {
    let value = run(
        "<html><body><div><p class='lead'>x</p><p>y</p></div></body></html>",
        "document.querySelector('p.lead:first-child') === null ? 'null' : 'matched'",
    )?;
    matches!(value, Value::String(ref s) if s == "matched")
        .then_some(())
        .ok_or_else(|| fail("expected 'p.lead:first-child' to match the first p with class lead"))
}

#[test]
fn pseudo_class_with_query_selector_all() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <section><p>a</p></section>
            <section><p>b</p><p>c</p></section>
            <section><p>d</p></section>
        </body></html>",
        "document.querySelectorAll('p:first-child').length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected three first-child p elements across the sections"))
}

#[test]
fn not_pseudo_class_excludes_class_match() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p class='skip'>x</p>
            <p id='want'>y</p>
            <p class='skip'>z</p>
        </body></html>",
        "document.querySelector('p:not(.skip)').id",
    )?;
    matches!(value, Value::String(ref s) if s == "want")
        .then_some(())
        .ok_or_else(|| fail("expected 'p:not(.skip)' to pick the p without the skip class"))
}

#[test]
fn not_pseudo_class_accepts_selector_list() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p>a</p>
            <h2>b</h2>
            <h3>c</h3>
            <div id='want'>d</div>
        </body></html>",
        "document.querySelector(':not(p, h2, h3)').tagName",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("body") || s.eq_ignore_ascii_case("div"))
        .then_some(())
        .ok_or_else(|| fail("expected ':not(p, h2, h3)' to skip those tags and pick body or div"))
}

#[test]
fn not_pseudo_class_with_query_selector_all() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p>a</p>
            <p class='skip'>b</p>
            <p>c</p>
            <p class='skip'>d</p>
        </body></html>",
        "document.querySelectorAll('p:not(.skip)').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected two p's without the skip class"))
}

#[test]
fn not_pseudo_class_with_tag_argument() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <section>
                <p>a</p>
                <h2 id='want'>b</h2>
            </section>
        </body></html>",
        "document.querySelector('section :not(p)').id",
    )?;
    matches!(value, Value::String(ref s) if s == "want")
        .then_some(())
        .ok_or_else(|| fail("expected 'section :not(p)' to pick the h2 inside section"))
}

#[test]
fn not_pseudo_class_via_matches() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p id='plain'>x</p>
            <p id='special' class='special'>y</p>
        </body></html>",
        "const plain = document.getElementById('plain');
        const special = document.getElementById('special');
        const ok = plain.matches(':not(.special)') === true &&
                   special.matches(':not(.special)') === false;
        ok ? 'ok' : 'wrong'",
    )?;
    matches!(value, Value::String(ref s) if s == "ok")
        .then_some(())
        .ok_or_else(|| fail("expected matches(':not(.special)') to invert class match"))
}

#[test]
fn not_pseudo_class_nested() -> Result<(), Error> {
    // :not(:not(.foo)) reduces to .foo via double negation.
    // The parser supports nested parens via find_balanced_close_paren.
    let value = run(
        "<html><body>
            <p>a</p>
            <p class='foo' id='want'>b</p>
            <p>c</p>
        </body></html>",
        "document.querySelector('p:not(:not(.foo))').id",
    )?;
    matches!(value, Value::String(ref s) if s == "want")
        .then_some(())
        .ok_or_else(|| fail("expected ':not(:not(.foo))' to fold to '.foo'"))
}

#[test]
fn not_pseudo_class_combines_with_other_constraints() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <a class='ext' href='/x'>x</a>
            <a class='int' id='want' href='/y'>y</a>
            <a class='ext' href='/z'>z</a>
        </body></html>",
        "document.querySelector('a[href]:not(.ext)').id",
    )?;
    matches!(value, Value::String(ref s) if s == "want")
        .then_some(())
        .ok_or_else(|| fail("expected 'a[href]:not(.ext)' to pick the non-ext link"))
}

#[test]
fn matches_with_pseudo_class() -> Result<(), Error> {
    let value = run(
        "<html><body><div><p id='first'>a</p><p id='middle'>b</p><p id='last'>c</p></div></body></html>",
        "const first = document.getElementById('first');
        const last = document.getElementById('last');
        const middle = document.getElementById('middle');
        const all_correct = first.matches(':first-child') === true &&
                            middle.matches(':first-child') === false &&
                            last.matches(':last-child') === true;
        all_correct ? 'ok' : 'wrong'",
    )?;
    matches!(value, Value::String(ref s) if s == "ok")
        .then_some(())
        .ok_or_else(|| fail("expected matches with pseudo-class to work for each position"))
}

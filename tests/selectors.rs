//! `querySelector` extended grammar (v0.7.8): multi-class
//! compounds, `[attr]` / `[attr=val]` / `[attr^=val]` /
//! `[attr$=val]` / `[attr*=val]`, `*`, descendant + child
//! combinators, selector lists.

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
fn multiple_classes_in_one_compound_must_all_match() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <div class='a' id='only-a'>x</div>
            <div class='a b' id='ab'>x</div>
            <div class='b c' id='bc'>x</div>
        </body></html>",
        "document.querySelector('div.a.b').id",
    )?;
    matches!(value, Value::String(ref s) if s == "ab")
        .then_some(())
        .ok_or_else(|| fail("expected only the 'a b'-class div to match .a.b"))
}

#[test]
fn universal_selector_matches_first_descendant() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'>x</div></body></html>",
        "document.querySelector('*').tagName",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("body"))
        .then_some(())
        .ok_or_else(|| {
            fail("expected universal selector to match the first descendant element (body)")
        })
}

#[test]
fn descendant_combinator_walks_through_intermediates() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <section><article><p id='target'>x</p></article></section>
        </body></html>",
        "document.querySelector('section p').id",
    )?;
    matches!(value, Value::String(ref s) if s == "target")
        .then_some(())
        .ok_or_else(|| fail("expected descendant combinator to skip the article wrapper"))
}

#[test]
fn child_combinator_requires_direct_parent() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <section>
                <article><p id='nested'>x</p></article>
                <p id='direct'>y</p>
            </section>
        </body></html>",
        "document.querySelector('section > p').id",
    )?;
    matches!(value, Value::String(ref s) if s == "direct")
        .then_some(())
        .ok_or_else(|| fail("expected 'section > p' to skip nested p and pick the direct child"))
}

#[test]
fn child_combinator_skips_indirect_descendants() -> Result<(), Error> {
    let value = run(
        "<html><body><div><span><i id='deep'>x</i></span></div></body></html>",
        "document.querySelector('div > i') === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected 'div > i' to NOT match a grandchild i"))
}

#[test]
fn attribute_existence_selector() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <a id='plain'>x</a>
            <a id='linked' href='/x'>y</a>
        </body></html>",
        "document.querySelector('a[href]').id",
    )?;
    matches!(value, Value::String(ref s) if s == "linked")
        .then_some(())
        .ok_or_else(|| fail("expected [href] existence selector to skip the linkless a"))
}

#[test]
fn attribute_equals_selector() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <input id='a' type='checkbox'/>
            <input id='b' type='radio'/>
        </body></html>",
        "document.querySelector('input[type=\"radio\"]').id",
    )?;
    matches!(value, Value::String(ref s) if s == "b")
        .then_some(())
        .ok_or_else(|| fail("expected [type=radio] equals selector to pick the radio input"))
}

#[test]
fn attribute_starts_with_selector() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <a id='internal' href='/about'>x</a>
            <a id='external' href='https://example.com'>y</a>
        </body></html>",
        "document.querySelector('a[href^=\"https\"]').id",
    )?;
    matches!(value, Value::String(ref s) if s == "external")
        .then_some(())
        .ok_or_else(|| fail("expected [href^=https] to match only the absolute URL"))
}

#[test]
fn attribute_ends_with_selector() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <img id='png' src='cat.png'/>
            <img id='jpg' src='dog.jpg'/>
        </body></html>",
        "document.querySelector('img[src$=\".png\"]').id",
    )?;
    matches!(value, Value::String(ref s) if s == "png")
        .then_some(())
        .ok_or_else(|| fail("expected [src$=.png] to match the png image"))
}

#[test]
fn attribute_contains_selector() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <div id='top' data-tag='headline-large'>x</div>
            <div id='mid' data-tag='caption'>y</div>
        </body></html>",
        "document.querySelector('[data-tag*=\"line\"]').id",
    )?;
    matches!(value, Value::String(ref s) if s == "top")
        .then_some(())
        .ok_or_else(|| fail("expected [data-tag*=line] substring match"))
}

#[test]
fn selector_list_matches_either_branch() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <h2 id='h2'>x</h2>
            <h3 id='h3'>y</h3>
        </body></html>",
        "document.querySelector('h2, h3').tagName",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("h2"))
        .then_some(())
        .ok_or_else(|| {
            fail("expected selector list to take the first descendant that matches either side")
        })
}

#[test]
fn selector_list_picks_second_when_first_misses() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p id='one'>x</p>
            <h2 id='two'>y</h2>
        </body></html>",
        "document.querySelector('header, p').id",
    )?;
    matches!(value, Value::String(ref s) if s == "one")
        .then_some(())
        .ok_or_else(|| fail("expected list to pick the p when no header is present"))
}

#[test]
fn compound_with_class_and_attribute() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <a id='one' class='link'>x</a>
            <a id='two' class='link' href='/x'>y</a>
        </body></html>",
        "document.querySelector('a.link[href]').id",
    )?;
    matches!(value, Value::String(ref s) if s == "two")
        .then_some(())
        .ok_or_else(|| fail("expected a.link[href] to combine class + attribute constraints"))
}

#[test]
fn nested_combinator_with_id_anchor() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <section id='outer'>
                <article>
                    <p id='target'>x</p>
                </article>
            </section>
            <section><p id='other'>y</p></section>
        </body></html>",
        "document.querySelector('#outer p').id",
    )?;
    matches!(value, Value::String(ref s) if s == "target")
        .then_some(())
        .ok_or_else(|| fail("expected '#outer p' to scope to the id=outer subtree"))
}

#[test]
fn adjacent_sibling_matches_immediately_following() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <h2 id='heading'>Title</h2>
            <p id='lead'>Lead paragraph</p>
            <p id='body'>Body paragraph</p>
        </body></html>",
        "document.querySelector('h2 + p').id",
    )?;
    matches!(value, Value::String(ref s) if s == "lead")
        .then_some(())
        .ok_or_else(|| fail("expected 'h2 + p' to pick the p immediately after the heading"))
}

#[test]
fn adjacent_sibling_skips_non_adjacent() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <h2>heading</h2>
            <div>spacer</div>
            <p id='not-adjacent'>p that's not adjacent</p>
        </body></html>",
        "document.querySelector('h2 + p') === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected 'h2 + p' to NOT match a p separated by a div"))
}

#[test]
fn adjacent_sibling_with_class_filter() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <h2>heading</h2>
            <p class='other'>x</p>
            <h3>another</h3>
            <p class='target' id='want'>y</p>
        </body></html>",
        "document.querySelector('h3 + p.target').id",
    )?;
    matches!(value, Value::String(ref s) if s == "want")
        .then_some(())
        .ok_or_else(|| fail("expected 'h3 + p.target' to pick the post-h3 p with class target"))
}

#[test]
fn general_sibling_matches_non_adjacent_following() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <h2>heading</h2>
            <div>spacer1</div>
            <div>spacer2</div>
            <p id='target'>finally a p</p>
        </body></html>",
        "document.querySelector('h2 ~ p').id",
    )?;
    matches!(value, Value::String(ref s) if s == "target")
        .then_some(())
        .ok_or_else(|| fail("expected 'h2 ~ p' to match p even with intermediate siblings"))
}

#[test]
fn general_sibling_does_not_cross_parent_boundary() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <h2>heading</h2>
            <section><p id='nested'>nested p</p></section>
        </body></html>",
        "document.querySelector('h2 ~ p') === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| {
            fail("expected 'h2 ~ p' to NOT cross into section's children (sibling = same parent)")
        })
}

#[test]
fn general_sibling_picks_first_match_in_document_order() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <h2>heading</h2>
            <p id='first'>first</p>
            <p id='second'>second</p>
            <p id='third'>third</p>
        </body></html>",
        "document.querySelector('h2 ~ p').id",
    )?;
    matches!(value, Value::String(ref s) if s == "first")
        .then_some(())
        .ok_or_else(|| {
            fail("expected querySelector to take the first match in document order")
        })
}

#[test]
fn query_selector_all_with_general_sibling() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <h2>heading</h2>
            <p>a</p>
            <p>b</p>
            <p>c</p>
        </body></html>",
        "document.querySelectorAll('h2 ~ p').length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected 'h2 ~ p' to match all three following sibling p"))
}

#[test]
fn query_selector_all_with_adjacent_sibling() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <h2>h1</h2>
            <p>p1</p>
            <h2>h2</h2>
            <p>p2</p>
            <h2>h3</h2>
            <p>p3</p>
        </body></html>",
        "document.querySelectorAll('h2 + p').length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected three h2+p adjacent-sibling matches"))
}

#[test]
fn sibling_combinator_chains_with_descendant() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <section>
                <h2>title</h2>
                <p id='want'>scoped</p>
            </section>
            <h2>other</h2>
            <p>not-scoped</p>
        </body></html>",
        "document.querySelector('section h2 + p').id",
    )?;
    matches!(value, Value::String(ref s) if s == "want")
        .then_some(())
        .ok_or_else(|| fail("expected 'section h2 + p' to chain descendant + adjacent-sibling"))
}

#[test]
fn no_match_returns_null() -> Result<(), Error> {
    let value = run(
        "<html><body><div></div></body></html>",
        "document.querySelector('.never') === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected no-match to return null"))
}

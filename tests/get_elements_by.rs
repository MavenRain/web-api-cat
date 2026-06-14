//! `getElementsByTagName(name)` and `getElementsByClassName(names)`
//! on both `Element` and `document` (v0.7.14).

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
fn get_elements_by_tag_name_returns_all_descendants() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p>a</p>
            <p>b</p>
            <div><p>c</p></div>
        </body></html>",
        "document.getElementsByTagName('p').length",
    )?;
    matches!(value, Value::Number(n) if (n - 3.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected three p descendants"))
}

#[test]
fn get_elements_by_tag_name_universal_matches_all() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <div></div>
            <p></p>
            <span></span>
        </body></html>",
        "document.getElementsByTagName('*').length",
    )?;
    matches!(value, Value::Number(n) if n >= 3.0)
        .then_some(())
        .ok_or_else(|| fail("expected '*' to match at least the three top-level body children"))
}

#[test]
fn get_elements_by_tag_name_is_case_insensitive() -> Result<(), Error> {
    let value = run(
        "<html><body><p>a</p><p>b</p></body></html>",
        "document.getElementsByTagName('P').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected case-insensitive match for 'P'"))
}

#[test]
fn get_elements_by_tag_name_empty_returns_empty() -> Result<(), Error> {
    let value = run(
        "<html><body><p>a</p></body></html>",
        "document.getElementsByTagName('').length",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected empty tag name to yield empty collection"))
}

#[test]
fn get_elements_by_tag_name_indexed_in_document_order() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <p id='one'>a</p>
            <p id='two'>b</p>
            <p id='three'>c</p>
        </body></html>",
        "const ps = document.getElementsByTagName('p');
        ps[0].id + ',' + ps[1].id + ',' + ps[2].id",
    )?;
    matches!(value, Value::String(ref s) if s == "one,two,three")
        .then_some(())
        .ok_or_else(|| fail("expected entries to land at numeric keys in document order"))
}

#[test]
fn element_get_elements_by_tag_name_scopes_to_subtree() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <section id='scope'><p>a</p><p>b</p></section>
            <p>outside</p>
        </body></html>",
        "document.getElementById('scope').getElementsByTagName('p').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected element-side getElementsByTagName to skip outside p"))
}

#[test]
fn get_elements_by_class_name_single_class() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <div class='target'>a</div>
            <div class='other'>b</div>
            <p class='target'>c</p>
        </body></html>",
        "document.getElementsByClassName('target').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected two elements with class 'target'"))
}

#[test]
fn get_elements_by_class_name_requires_all_classes() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <div class='alpha'>a</div>
            <div class='alpha beta'>b</div>
            <div class='alpha beta gamma'>c</div>
            <div class='beta'>d</div>
        </body></html>",
        "document.getElementsByClassName('alpha beta').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected two elements with BOTH alpha and beta classes"))
}

#[test]
fn get_elements_by_class_name_empty_returns_empty() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <div class='foo'>a</div>
            <div>b</div>
        </body></html>",
        "document.getElementsByClassName('').length",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected empty class list to yield empty (not match-all)"))
}

#[test]
fn get_elements_by_class_name_whitespace_only_returns_empty() -> Result<(), Error> {
    let value = run(
        "<html><body><div class='foo'>x</div></body></html>",
        "document.getElementsByClassName('   ').length",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected whitespace-only class list to yield empty"))
}

#[test]
fn element_get_elements_by_class_name_scopes_to_subtree() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <div id='scope'>
                <p class='want'>a</p>
                <p class='want'>b</p>
            </div>
            <p class='want'>c</p>
        </body></html>",
        "document.getElementById('scope').getElementsByClassName('want').length",
    )?;
    matches!(value, Value::Number(n) if (n - 2.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected element.getElementsByClassName to skip outside matches"))
}

#[test]
fn get_elements_by_tag_name_iteration_via_for_loop() -> Result<(), Error> {
    let value = run(
        "<html><body><p>a</p><p>b</p><p>c</p></body></html>",
        "const ps = document.getElementsByTagName('p');
        let acc = '';
        for (let i = 0; i < ps.length; i = i + 1) {
            acc = acc + ps[i].textContent;
        }
        acc",
    )?;
    matches!(value, Value::String(ref s) if s == "abc")
        .then_some(())
        .ok_or_else(|| fail("expected for-loop iteration to visit textContent in document order"))
}

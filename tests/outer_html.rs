//! `Element.outerHTML` getter (v0.6.7): serialises `<tag attrs>
//! inner</tag>` for the element itself (not just its children).

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
fn outer_html_includes_own_tag_for_leaf() -> Result<(), Error> {
    let value = run(
        "<html><body><span id='host'>hi</span></body></html>",
        "document.getElementById('host').outerHTML",
    )?;
    matches!(value, Value::String(ref s) if s == "<span id=\"host\">hi</span>")
        .then_some(())
        .ok_or_else(|| fail("expected outerHTML to wrap with own tag and attributes"))
}

#[test]
fn outer_html_recurses_for_children() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>x</a><b>y</b></div></body></html>",
        "document.getElementById('host').outerHTML",
    )?;
    matches!(value, Value::String(ref s) if s == "<div id=\"host\"><a>x</a><b>y</b></div>")
        .then_some(())
        .ok_or_else(|| fail("expected outerHTML to recurse into children"))
}

#[test]
fn outer_html_setter_leaves_host_internal_children_intact() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>x</a></div></body></html>",
        "const host = document.getElementById('host');
        host.outerHTML = '<p>nope</p>';
        host.children[0].tagName",
    )?;
    matches!(value, Value::String(ref s) if s == "a")
        .then_some(())
        .ok_or_else(|| fail("expected host's own children to stay attached to host after detach"))
}

#[test]
fn outer_html_setter_replaces_self_in_parent_children() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>x</a></div></body></html>",
        "const host = document.getElementById('host');
        const body = document.body;
        host.outerHTML = '<p id=\"swap\">new</p>';
        body.children[0].tagName + ',' + body.children[0].getAttribute('id')",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("p,swap"))
        .then_some(())
        .ok_or_else(|| fail("expected outerHTML setter to splice parsed fragment into parent"))
}

#[test]
fn outer_html_setter_detaches_self() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><a>x</a></div></body></html>",
        "const host = document.getElementById('host');
        host.outerHTML = '<p>new</p>';
        host.parentElement === null ? 'detached' : 'attached'",
    )?;
    matches!(value, Value::String(ref s) if s == "detached")
        .then_some(())
        .ok_or_else(|| fail("expected outerHTML setter to clear self.parentElement"))
}

#[test]
fn outer_html_setter_handles_multi_node_fragment() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='middle'>m</div></body></html>",
        "const middle = document.getElementById('middle');
        middle.outerHTML = '<a>a</a><b>b</b>';
        const body = document.body;
        body.children.length + ',' + body.children[0].tagName + ',' + body.children[1].tagName",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("2,a,b"))
        .then_some(())
        .ok_or_else(|| fail("expected outerHTML setter to splice multiple parsed roots"))
}

#[test]
fn outer_html_setter_preserves_siblings() -> Result<(), Error> {
    let value = run(
        "<html><body><a>before</a><div id='middle'>m</div><c>after</c></body></html>",
        "const middle = document.getElementById('middle');
        middle.outerHTML = '<b>new</b>';
        const body = document.body;
        body.children[0].tagName + ',' + body.children[1].tagName + ',' + body.children[2].tagName",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("a,b,c"))
        .then_some(())
        .ok_or_else(|| fail("expected siblings to stay in place around the splice"))
}

#[test]
fn outer_html_setter_noop_when_detached() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const el = document.createElement('div');
        el.outerHTML = '<p>nope</p>';
        el.tagName",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("div"))
        .then_some(())
        .ok_or_else(|| fail("expected outerHTML setter to no-op on detached element"))
}

#[test]
fn outer_html_works_on_created_element() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "const el = document.createElement('p');
        el.setAttribute('id', 'src');
        el.outerHTML",
    )?;
    matches!(value, Value::String(ref s) if s == "<p id=\"src\"></p>")
        .then_some(())
        .ok_or_else(|| fail("expected outerHTML on createElement output"))
}

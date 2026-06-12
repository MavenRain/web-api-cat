//! `Element.parentElement` / `parentNode` / `Element.remove()`
//! (v0.6.8): hidden `__parent__` backref maintained by every
//! structural mutator.

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
fn parent_element_of_parsed_child_is_its_parent() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span id='leaf'>x</span></div></body></html>",
        "document.getElementById('leaf').parentElement.tagName",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("div"))
        .then_some(())
        .ok_or_else(|| fail("expected leaf's parentElement to be div"))
}

#[test]
fn parent_element_of_root_is_null() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "document.documentElement.parentElement === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected root parentElement to be null"))
}

#[test]
fn parent_element_of_created_element_is_null() -> Result<(), Error> {
    let value = run(
        "<html><body></body></html>",
        "document.createElement('span').parentElement === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected createElement output's parentElement to be null"))
}

#[test]
fn append_child_sets_parent_element_on_child() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'></div></body></html>",
        "const host = document.getElementById('host');
        const child = document.createElement('span');
        host.appendChild(child);
        child.parentElement.tagName",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("div"))
        .then_some(())
        .ok_or_else(|| fail("expected appendChild to set child.parentElement"))
}

#[test]
fn remove_child_clears_parent_element() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span id='leaf'>x</span></div></body></html>",
        "const host = document.getElementById('host');
        const leaf = document.getElementById('leaf');
        host.removeChild(leaf);
        leaf.parentElement === null ? 'null' : 'not-null'",
    )?;
    matches!(value, Value::String(ref s) if s == "null")
        .then_some(())
        .ok_or_else(|| fail("expected removeChild to clear child.parentElement"))
}

#[test]
fn insert_before_sets_parent_element() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span id='ref'>r</span></div></body></html>",
        "const host = document.getElementById('host');
        const ref_node = document.getElementById('ref');
        const fresh = document.createElement('a');
        host.insertBefore(fresh, ref_node);
        fresh.parentElement.tagName",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("div"))
        .then_some(())
        .ok_or_else(|| fail("expected insertBefore to set new.parentElement"))
}

#[test]
fn replace_child_swaps_parent_backrefs() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span id='old'>x</span></div></body></html>",
        "const host = document.getElementById('host');
        const old_child = document.getElementById('old');
        const new_child = document.createElement('a');
        host.replaceChild(new_child, old_child);
        const new_parent = new_child.parentElement;
        const new_tag = new_parent === null ? 'detached' : new_parent.tagName;
        const old_parent = old_child.parentElement;
        const old_tag = old_parent === null ? 'detached' : old_parent.tagName;
        new_tag + ',' + old_tag",
    )?;
    matches!(value, Value::String(ref s) if s.eq_ignore_ascii_case("div,detached"))
        .then_some(())
        .ok_or_else(|| fail("expected replaceChild to attach new + detach old"))
}

#[test]
fn element_remove_detaches_from_parent() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span id='leaf'>x</span></div></body></html>",
        "const host = document.getElementById('host');
        const leaf = document.getElementById('leaf');
        leaf.remove();
        const child_count = host.children.length;
        const detached = leaf.parentElement === null ? 1 : 0;
        child_count * 10 + detached",
    )?;
    matches!(value, Value::Number(n) if (n - 1.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| {
            fail("expected leaf.remove() to drop from parent.children AND clear leaf.parentElement")
        })
}

#[test]
fn parent_node_matches_parent_element() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span id='leaf'>x</span></div></body></html>",
        "const leaf = document.getElementById('leaf');
        leaf.parentNode === leaf.parentElement ? 'same' : 'different'",
    )?;
    matches!(value, Value::String(ref s) if s == "same")
        .then_some(())
        .ok_or_else(|| fail("expected parentNode === parentElement"))
}

#[test]
fn inner_html_setter_clears_old_parent_backrefs_and_sets_new() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span id='old'>x</span></div></body></html>",
        "const host = document.getElementById('host');
        const old_child = document.getElementById('old');
        host.innerHTML = '<a>fresh</a>';
        const new_first = host.children[0];
        const old_detached = old_child.parentElement === null ? 1 : 0;
        const new_attached = new_first.parentElement === host ? 1 : 0;
        old_detached * 10 + new_attached",
    )?;
    matches!(value, Value::Number(n) if (n - 11.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected innerHTML setter to swap parent backrefs"))
}

#[test]
fn deep_clone_grandchild_parent_points_at_cloned_intermediate() -> Result<(), Error> {
    let value = run(
        "<html><body><div id='host'><span><a>x</a></span></div></body></html>",
        "const host = document.getElementById('host');
        const clone = host.cloneNode(true);
        const cloned_inter = clone.children[0];
        const cloned_leaf = cloned_inter.children[0];
        cloned_leaf.parentElement === cloned_inter ? 'attached-to-clone' : 'attached-to-original'",
    )?;
    matches!(value, Value::String(ref s) if s == "attached-to-clone")
        .then_some(())
        .ok_or_else(|| {
            fail("expected deep clone's grandchild.parentElement to point at the cloned intermediate")
        })
}

//! v0.7.20 image-element accessor pack: `src`, `alt`, `width`,
//! `height` (attribute reflection) and `naturalWidth`,
//! `naturalHeight`, `complete` (hidden-slot reflection
//! populated by the downstream consumer).

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
fn img_src_getter_reflects_attribute() -> Result<(), Error> {
    let value = run(
        "<html><body><img id='host' src='/cat.png'/></body></html>",
        "document.getElementById('host').src",
    )?;
    matches!(value, Value::String(ref s) if s == "/cat.png")
        .then_some(())
        .ok_or_else(|| fail("expected img.src to reflect the src attribute"))
}

#[test]
fn img_src_setter_round_trips() -> Result<(), Error> {
    let value = run(
        "<html><body><img id='host' src='/cat.png'/></body></html>",
        "const el = document.getElementById('host');
        el.src = '/dog.png';
        el.getAttribute('src')",
    )?;
    matches!(value, Value::String(ref s) if s == "/dog.png")
        .then_some(())
        .ok_or_else(|| fail("expected img.src setter to round-trip via getAttribute"))
}

#[test]
fn img_alt_getter_reflects_attribute() -> Result<(), Error> {
    let value = run(
        "<html><body><img id='host' alt='a cat' src='/cat.png'/></body></html>",
        "document.getElementById('host').alt",
    )?;
    matches!(value, Value::String(ref s) if s == "a cat")
        .then_some(())
        .ok_or_else(|| fail("expected img.alt to reflect the alt attribute"))
}

#[test]
fn img_width_getter_parses_attribute() -> Result<(), Error> {
    let value = run(
        "<html><body><img id='host' width='320' src='/cat.png'/></body></html>",
        "document.getElementById('host').width",
    )?;
    matches!(value, Value::Number(n) if (n - 320.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected img.width to parse the width attribute"))
}

#[test]
fn img_width_defaults_to_zero_when_absent() -> Result<(), Error> {
    let value = run(
        "<html><body><img id='host' src='/cat.png'/></body></html>",
        "document.getElementById('host').width",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected img.width to default to 0"))
}

#[test]
fn img_height_setter_writes_attribute() -> Result<(), Error> {
    let value = run(
        "<html><body><img id='host' src='/cat.png'/></body></html>",
        "const el = document.getElementById('host');
        el.height = 240;
        el.getAttribute('height')",
    )?;
    matches!(value, Value::String(ref s) if s == "240")
        .then_some(())
        .ok_or_else(|| fail("expected img.height setter to write the height attribute"))
}

#[test]
fn img_natural_width_defaults_to_zero() -> Result<(), Error> {
    // Before the downstream consumer populates __natural_width,
    // the getter should return 0 (image not yet decoded).
    let value = run(
        "<html><body><img id='host' src='/cat.png'/></body></html>",
        "document.getElementById('host').naturalWidth",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected img.naturalWidth to default to 0"))
}

#[test]
fn img_natural_height_defaults_to_zero() -> Result<(), Error> {
    let value = run(
        "<html><body><img id='host' src='/cat.png'/></body></html>",
        "document.getElementById('host').naturalHeight",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected img.naturalHeight to default to 0"))
}

#[test]
fn img_complete_defaults_to_false() -> Result<(), Error> {
    let value = run(
        "<html><body><img id='host' src='/cat.png'/></body></html>",
        "document.getElementById('host').complete",
    )?;
    matches!(value, Value::Boolean(false))
        .then_some(())
        .ok_or_else(|| fail("expected img.complete to default to false before load"))
}

#[test]
fn img_natural_size_setter_is_noop() -> Result<(), Error> {
    // Assigning naturalWidth from JS should silently do nothing
    // (read-only property with a no-op setter so the script
    // doesn't throw).  The getter still returns the slot value
    // -- 0 since nothing has set it.
    let value = run(
        "<html><body><img id='host' src='/cat.png'/></body></html>",
        "const el = document.getElementById('host');
        el.naturalWidth = 999;
        el.naturalWidth",
    )?;
    matches!(value, Value::Number(n) if (n - 0.0).abs() < 1e-9)
        .then_some(())
        .ok_or_else(|| fail("expected img.naturalWidth setter to be a no-op"))
}

#[test]
fn img_query_selectable_by_src() -> Result<(), Error> {
    let value = run(
        "<html><body>
            <img src='/a.png'/>
            <img id='want' src='/b.png'/>
            <img src='/c.png'/>
        </body></html>",
        "document.querySelector('img[src=\"/b.png\"]').id",
    )?;
    matches!(value, Value::String(ref s) if s == "want")
        .then_some(())
        .ok_or_else(|| fail("expected attribute selector to find the matching img"))
}

#[test]
fn img_accessors_install_on_created_element() -> Result<(), Error> {
    // createElement('img') gets the accessor pack too (third
    // build site in document.rs).
    let value = run(
        "<html><body></body></html>",
        "const img = document.createElement('img');
        img.src = '/created.png';
        img.alt = 'made by JS';
        img.width = 100;
        const ok = img.src === '/created.png' &&
                   img.alt === 'made by JS' &&
                   img.width === 100 &&
                   img.complete === false;
        ok ? 'ok' : 'wrong'",
    )?;
    matches!(value, Value::String(ref s) if s == "ok")
        .then_some(())
        .ok_or_else(|| fail("expected createElement('img') to expose the accessor pack"))
}

#[test]
fn non_img_elements_see_harmless_defaults() -> Result<(), Error> {
    // The accessor pack installs on every element, so a <div>
    // sees harmless defaults: empty strings, 0, false.
    let value = run(
        "<html><body><div id='host'>x</div></body></html>",
        "const el = document.getElementById('host');
        const ok = el.src === '' &&
                   el.alt === '' &&
                   el.width === 0 &&
                   el.naturalWidth === 0 &&
                   el.complete === false;
        ok ? 'ok' : 'wrong'",
    )?;
    matches!(value, Value::String(ref s) if s == "ok")
        .then_some(())
        .ok_or_else(|| fail("expected non-img elements to see harmless defaults"))
}

#[test]
fn set_natural_size_helper_round_trips_to_js() -> Result<(), Error> {
    // The downstream consumer (tauri-runtime-servocat) calls
    // image::set_natural_size after decoding.  This test proves
    // the JS-side getter sees the populated slot.
    let html_doc = html_cat::parse("<html><body><img id='host' src='/cat.png'/></body></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html_doc);

    let tokens = lex("document.getElementById('host')").map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (img_value, heap) = evaluate_program_with(&program, env.clone(), heap, Fuel::new(50_000))
        .map_err(Error::from)?;

    let heap = web_api_cat::image::set_natural_size(&img_value, 640, 480, heap);
    let heap = web_api_cat::image::set_complete(&img_value, true, heap);

    let tokens2 = lex("const el = document.getElementById('host');
        '' + el.naturalWidth + ',' + el.naturalHeight + ',' + el.complete")
    .map_err(boa_cat::Error::from)?;
    let program2 = parse_script(&tokens2).map_err(boa_cat::Error::from)?;
    let (result, _heap) =
        evaluate_program_with(&program2, env, heap, Fuel::new(50_000)).map_err(Error::from)?;
    matches!(result, Value::String(ref s) if s == "640,480,true")
        .then_some(())
        .ok_or_else(|| fail("expected set_natural_size + set_complete to flow through JS getters"))
}

#[test]
fn img_innerhtml_preserves_attributes_after_round_trip() -> Result<(), Error> {
    // Sanity: after the image-accessor install, a parsed img
    // round-trips through innerHTML without losing its attrs.
    let value = run(
        "<html><body><img id='host' src='/cat.png' alt='cat' width='100' height='80'/></body></html>",
        "document.body.innerHTML",
    )?;
    // The serialisation order is BTreeMap-deterministic across
    // attribute keys, so we test for substring presence rather
    // than exact equality (forward-compat with future attrs).
    matches!(value, Value::String(ref s) if
        s.contains("src=\"/cat.png\"")
        && s.contains("alt=\"cat\"")
        && s.contains("width=\"100\"")
        && s.contains("height=\"80\"")
    )
    .then_some(())
    .ok_or_else(|| fail("expected img attrs to survive innerHTML round-trip"))
}

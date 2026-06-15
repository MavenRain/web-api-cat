//! v0.7.21 public helpers: `document_root_object_id` extracts
//! the document root `ObjectId` from an env produced by
//! [`web_api_cat::install`]; `find_all_in_document` queries the
//! whole document tree (root + descendants) without going
//! through the JS engine.  Downstream consumers (e.g.
//! tauri-runtime-servocat populating decoded `<img>` metadata)
//! use these to walk the heap directly.
//!
//! [`web_api_cat::install`]: web_api_cat::install

use boa_cat::env::Env;
use boa_cat::heap::Heap;
use boa_cat::value::Value;
use web_api_cat::Error;
use web_api_cat::element::{document_root_object_id, find_all_in_document, read_attribute};

fn fail(_msg: &'static str) -> Error {
    Error::Engine(boa_cat::Error::Unsupported { feature: "test" })
}

#[test]
fn document_root_returns_some_after_install() -> Result<(), Error> {
    let doc = html_cat::parse("<html><body><p>x</p></body></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &doc);
    document_root_object_id(&env, &heap)
        .is_some()
        .then_some(())
        .ok_or_else(|| fail("expected a document root after install"))
}

#[test]
fn document_root_returns_none_for_empty_env() -> Result<(), Error> {
    let heap = Heap::new();
    let env = Env::empty();
    document_root_object_id(&env, &heap)
        .is_none()
        .then_some(())
        .ok_or_else(|| fail("expected None when document binding is missing"))
}

#[test]
fn find_all_in_document_returns_img_elements() -> Result<(), Error> {
    let doc = html_cat::parse(
        "<html><body>
            <img id='a' src='/a.png'/>
            <img id='b' src='/b.png'/>
            <p>not an img</p>
        </body></html>",
    )?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &doc);
    let imgs = find_all_in_document(&env, "img", &heap);
    (imgs.len() == 2)
        .then_some(())
        .ok_or_else(|| fail("expected exactly two img elements"))
}

#[test]
fn read_attribute_returns_src_from_object_id() -> Result<(), Error> {
    let doc = html_cat::parse("<html><body><img src='/cat.png' alt='a cat'/></body></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &doc);
    let imgs = find_all_in_document(&env, "img", &heap);
    let img_id = *imgs.first().ok_or_else(|| fail("no img found"))?;
    let src = read_attribute(&Value::Object(img_id), "src", &heap)
        .ok_or_else(|| fail("expected src attribute"))?;
    let alt = read_attribute(&Value::Object(img_id), "alt", &heap)
        .ok_or_else(|| fail("expected alt attribute"))?;
    (src == "/cat.png" && alt == "a cat")
        .then_some(())
        .ok_or_else(|| fail("attribute values wrong"))
}

#[test]
fn find_all_in_document_returns_empty_for_missing_match() -> Result<(), Error> {
    let doc = html_cat::parse("<html><body><p>x</p></body></html>")?;
    let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &doc);
    let imgs = find_all_in_document(&env, "img", &heap);
    imgs.is_empty()
        .then_some(())
        .ok_or_else(|| fail("expected no img elements"))
}

#[test]
fn find_all_in_document_returns_empty_for_empty_env() -> Result<(), Error> {
    let heap = Heap::new();
    let env = Env::empty();
    let imgs = find_all_in_document(&env, "img", &heap);
    imgs.is_empty()
        .then_some(())
        .ok_or_else(|| fail("expected empty result with no document binding"))
}

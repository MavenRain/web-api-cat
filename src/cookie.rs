//! `document.cookie` bridge between the host's cookie jar and JS.
//!
//! Scripts read and write a single string property on the document
//! object whose name is `cookie`.  The host is responsible for
//! serializing the visible portion of its jar into the property
//! before each evaluation and parsing any change written by JS back
//! out after evaluation.  See
//! [`set_document_cookie`] / [`get_document_cookie`].
//!
//! Visibility (e.g. the `HttpOnly` filter on the read side) lives in
//! the host, not here: this crate only shuttles a string in and out.
//!
//! # Examples
//!
//! ```
//! # fn main() -> Result<(), web_api_cat::Error> {
//! use boa_cat::heap::Heap;
//! use web_api_cat::{document, get_document_cookie, set_document_cookie};
//!
//! let html = html_cat::parse("<html><body></body></html>")?;
//! let (document_value, _root, heap) = document::build(&html, Heap::new());
//! let heap = set_document_cookie(&document_value, heap, "session=abc");
//! let snapshot = get_document_cookie(&document_value, &heap);
//! assert_eq!(snapshot.as_deref(), Some("session=abc"));
//! # Ok(())
//! # }
//! ```

use boa_cat::Value;
use boa_cat::heap::Heap;
use boa_cat::value::ObjectId;

/// Property key under which the cookie string lives on the document
/// object.  Exposed so callers wiring `Object.defineProperty`-style
/// shims can name the slot consistently.
pub const PROPERTY_KEY: &str = "cookie";

/// Overwrite `document.cookie` with `cookies` and return the updated
/// heap.  When `document_value` is not an object, or the object has
/// been evicted from `heap`, the heap is returned unchanged.
///
/// The host is expected to call this once per evaluation, before the
/// script runs, with the already-filtered cookie string (i.e. with
/// `HttpOnly` cookies removed).
#[must_use]
pub fn set_document_cookie(document_value: &Value, heap: Heap, cookies: &str) -> Heap {
    let pair =
        object_id_of(document_value).and_then(|id| heap.object(id).cloned().map(|obj| (id, obj)));
    let cookies_owned = cookies.to_owned();
    pair.into_iter().fold(heap, |heap, (id, obj)| {
        let updated = obj.with(
            PROPERTY_KEY.to_owned(),
            Value::String(cookies_owned.clone()),
        );
        heap.store_object(id, updated).unwrap_or_else(|h| h)
    })
}

/// Read whatever string JS last assigned to `document.cookie`.
/// Returns `None` when `document_value` is not an object, the object
/// has been evicted, the `cookie` property is missing, or its value
/// is not a string.
#[must_use]
pub fn get_document_cookie(document_value: &Value, heap: &Heap) -> Option<String> {
    let id = object_id_of(document_value)?;
    let obj = heap.object(id)?;
    obj.get(PROPERTY_KEY).and_then(|value| match value {
        Value::String(s) => Some(s.clone()),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::Object(_)
        | Value::Function(_)
        | Value::Native(_) => None,
    })
}

fn object_id_of(value: &Value) -> Option<ObjectId> {
    match value {
        Value::Object(id) => Some(*id),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Function(_)
        | Value::Native(_) => None,
    }
}

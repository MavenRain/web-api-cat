//! Bridge between boa-cat (JS engine) and the DOM + fetch.
//!
//! # Example
//!
//! ```
//! # fn main() -> Result<(), web_api_cat::Error> {
//! use boa_cat::{evaluate_program_with, env::Env, fuel::Fuel, heap::Heap};
//! use ecma_lex_cat::lex;
//! use ecma_parse_cat::parse_script;
//!
//! let html = html_cat::parse("<html><body><p id='g'>hi</p></body></html>")?;
//! let (env, heap) = web_api_cat::install(Env::empty(), Heap::new(), &html);
//! let tokens = lex("document.getElementById('g').textContent")
//!     .map_err(boa_cat::Error::from)?;
//! let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
//! let (value, _heap) = evaluate_program_with(&program, env, heap, Fuel::new(100_000))?;
//! assert_eq!(format!("{value}"), "\"hi\"");
//! # Ok(())
//! # }
//! ```

#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![allow(clippy::similar_names)]

pub mod cookie;
pub mod document;
pub mod element;
pub mod error;
pub mod event;
pub mod extract;
pub mod fetch;
pub mod inline_style;
pub mod inner_html;
pub mod input;
pub mod install;
pub mod storage;

pub use cookie::{
    get_document_cookie, install_cookie_accessor, read_cookie_writes, set_document_cookie,
};
pub use error::Error;
pub use extract::extract_document;
pub use install::install;
pub use storage::{
    build_storage_object, lookup_local_storage, lookup_session_storage, read_storage_items,
    seed_storage,
};

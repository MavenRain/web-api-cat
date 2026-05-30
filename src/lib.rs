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

pub mod document;
pub mod element;
pub mod error;
pub mod fetch;
pub mod install;

pub use error::Error;
pub use install::install;

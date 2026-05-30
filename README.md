# web-api-cat

Bindings between [`boa-cat`](https://crates.io/crates/boa-cat) (JS engine) and [`html-cat`](https://crates.io/crates/html-cat) (DOM) + [`net-cat`](https://crates.io/crates/net-cat) (HTTP), exposing `document`, `Element` methods, and `fetch` as boa-cat `NativeFn`s so scripts can read/mutate a parsed HTML document and make synchronous HTTP requests.

`web-api-cat` is the seventh sub-crate of a `comp-cat-rs` Servo-replacement webview runtime targeting Tauri integration.  Same framework constraints as the rest of the stack: no `mut`, no `Rc`/`Arc`, no interior mutability, no panics.

## Example

```rust
use boa_cat::{evaluate_program_with, fuel::Fuel, env::Env, heap::Heap};
use ecma_lex_cat::lex;
use ecma_parse_cat::parse_script;
use web_api_cat::install;

fn main() -> Result<(), web_api_cat::Error> {
    let html = html_cat::parse("<html><body><p id='greet'>hello</p></body></html>")?;
    let (env, heap) = install(Env::empty(), Heap::new(), &html);
    let tokens = lex("document.getElementById('greet').textContent")
        .map_err(boa_cat::Error::from)?;
    let program = parse_script(&tokens).map_err(boa_cat::Error::from)?;
    let (value, _heap) = evaluate_program_with(&program, env, heap, Fuel::new(100_000))?;
    assert_eq!(format!("{value}"), "\"hello\"");
    Ok(())
}
```

## v0 scope

- `document.getElementById(id)` -- walks the document tree, returns the matching element-object or `null`.
- `document.querySelector(selector)` -- limited subset (`tag`, `.class`, `#id`, `tag.class`, `tag#id`).
- Element properties: `tagName`, `id`, `className`, `textContent`, `children` array.
- Element methods: `getAttribute(name)`, `setAttribute(name, value)`, `hasAttribute(name)`.
- `fetch(url)` -- synchronous net-cat call returning `{ ok, status, statusText, text, headers }`.

## Deferred to v0.2+

- Full CSS selector syntax in `querySelector`.
- DOM mutation back-propagation to layout-cat / paint-cat.
- Async `fetch` (Promise-based).
- DOM mutation API (`appendChild`, `removeChild`, ...).
- Computed style introspection.
- Event API.
- HTTPS support (waits on net-cat's TLS feature).

## License

MIT OR Apache-2.0

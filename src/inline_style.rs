//! Inline `style="..."` parsing and serialization (v0.7.2).
//!
//! Build-time: `crate::document::build_element` calls
//! [`parse_inline_style`] on the `style` attribute (if present)
//! and pre-populates `el.style` with the resulting
//! camelCase-keyed properties.  So `<div style="color: red">`
//! gives `el.style.color === 'red'` and
//! `<div style="font-size: 14px">` gives
//! `el.style.fontSize === '14px'`.
//!
//! Extract-time: [`crate::extract::extract_document`] walks the
//! style Object's property list, calls [`serialize_inline_style`]
//! to render it back to a kebab-case `style="..."` string, and
//! writes that into the dom-cat element's attribute list.  So a
//! JS-side `el.style.color = 'blue'` shows up in the
//! post-eval dom-cat tree as `style="color: blue"`.
//!
//! Limitations (v0):
//!
//! - Property values are stored verbatim; no normalisation
//!   (`'red'` vs `'#ff0000'` round-trip as-is).
//! - No vendor-prefix support (`WebkitTransform` /
//!   `MozTransform` would map to `-webkit-transform` /
//!   `-moz-transform` per spec; this module's `camel_to_kebab`
//!   doesn't special-case the leading uppercase, so vendor
//!   props end up as lowercase only).
//! - Single-colon split on the first `:`, so values containing
//!   `:` (e.g. `background: url(http://...)`) are preserved
//!   correctly by leaving everything after the first colon as
//!   the value.

/// Parse a `style="..."` attribute value into camelCase-keyed
/// properties.  Splits on `;`, then on the first `:`, trims, and
/// converts each kebab-case key into camelCase so JS dot-notation
/// (`el.style.fontSize`) works.  Returns the properties in source
/// order; the caller is responsible for de-duplication (last-write
/// wins per the CSS cascade).
#[must_use]
pub fn parse_inline_style(text: &str) -> Vec<(String, String)> {
    text.split(';')
        .filter_map(|decl| {
            let trimmed = decl.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .filter_map(|decl| decl.split_once(':'))
        .map(|(name, value)| (kebab_to_camel(name.trim()), value.trim().to_owned()))
        .collect()
}

/// Serialize `properties` back to a kebab-case `style="..."`
/// string suitable for dom-cat's attribute list.  Renders each
/// pair as `key: value` joined by `"; "`; returns an empty string
/// for an empty input so the caller can drop the `style`
/// attribute entirely.
#[must_use]
pub fn serialize_inline_style(properties: &[(String, String)]) -> String {
    properties
        .iter()
        .map(|(name, value)| format!("{}: {value}", camel_to_kebab(name)))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Convert `kebab-case` to `camelCase`.  First segment lowercased
/// in full; each subsequent dash-separated segment has its first
/// character upper-cased and the rest lower-cased.  Made `pub`
/// in v0.7.7 so `crate::document`'s dataset builder can reuse the
/// algorithm against `data-*` attribute suffixes.
#[must_use]
pub fn kebab_to_camel(kebab: &str) -> String {
    kebab
        .split('-')
        .enumerate()
        .map(|(i, segment)| {
            if i == 0 {
                segment.to_ascii_lowercase()
            } else {
                capitalize_first(segment)
            }
        })
        .collect()
}

fn camel_to_kebab(camel: &str) -> String {
    camel
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if c.is_ascii_uppercase() && i > 0 {
                format!("-{}", c.to_ascii_lowercase())
            } else {
                c.to_ascii_lowercase().to_string()
            }
        })
        .collect()
}

fn capitalize_first(s: &str) -> String {
    s.chars()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

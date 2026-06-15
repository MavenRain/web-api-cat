//! `Element`-side native methods: `getAttribute`, `setAttribute`,
//! `hasAttribute`, `querySelector`, plus the v0.6.1 / v0.6.2 / v0.6.3 /
//! v0.6.7 / v0.6.8 structural, class-list, and parent-tracking
//! mutators (`appendChild`, `removeChild`, `insertBefore`,
//! `replaceChild`, `classList.add`, `classList.remove`,
//! `classList.contains`, `classList.toggle`, `remove`).
//!
//! v0.6.8 introduces a hidden `__parent__` slot on every element
//! (`Value::Object(parent_id)` when attached, `Value::Null` when
//! detached) and routes every structural mutator through
//! `set_parent_backref` / `clear_parent_backref` so the slot
//! stays in sync with the actual children-array membership.
//!
//! Element objects are boa-cat [`Object`]s carrying these properties:
//!
//! - `tagName`: lowercased tag name (string).
//! - `id`: id attribute or empty string.
//! - `className`: class attribute or empty string.
//! - `textContent`: concatenated text of all descendant text nodes.
//! - `children`: array of child element objects.
//! - `__attributes`: object mapping attribute name -> value.
//! - `getAttribute`, `setAttribute`, `hasAttribute`,
//!   `querySelector`: native callables.

use std::collections::BTreeMap;

use boa_cat::Value;
use boa_cat::fuel::Fuel;
use boa_cat::heap::Heap;
use boa_cat::outcome::{EvalResult, Outcome};
use boa_cat::value::{Object, ObjectId};

/// `Element.getAttribute(name)` native implementation.
///
/// # Errors
///
/// Never returns `Err`; bad input yields `Value::Null`.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
pub fn get_attribute_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let name = string_arg(&args, 0);
    let outcome = read_attribute(&this, &name, &heap).map_or(Outcome::Normal(Value::Null), |v| {
        Outcome::Normal(Value::String(v))
    });
    Ok((outcome, heap, fuel))
}

/// `Element.hasAttribute(name)` native implementation.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
pub fn has_attribute_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let name = string_arg(&args, 0);
    let present = read_attribute(&this, &name, &heap).is_some();
    Ok((Outcome::Normal(Value::Boolean(present)), heap, fuel))
}

/// `Element.setAttribute(name, value)` native implementation.
///
/// # Errors
///
/// Never returns `Err`; missing element / attributes object yields the
/// unchanged heap.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
pub fn set_attribute_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let name = string_arg(&args, 0);
    let value = string_arg(&args, 1);
    let new_heap = write_attribute(&this, &name, &value, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
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
        | Value::Native(_)
        | Value::Promise(_) => None,
    }
}

fn attributes_id_of(element: &Object) -> Option<ObjectId> {
    match element.get("__attributes") {
        Some(value) => object_id_of(value),
        None => None,
    }
}

fn read_attribute(this: &Value, name: &str, heap: &Heap) -> Option<String> {
    let element_id = object_id_of(this)?;
    let element = heap.object(element_id)?;
    let attrs_id = attributes_id_of(element)?;
    let attrs = heap.object(attrs_id)?;
    match attrs.get(name) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) | None => None,
    }
}

fn write_attribute(this: &Value, name: &str, value: &str, heap: Heap) -> Heap {
    let Some(element_id) = object_id_of(this) else {
        return heap;
    };
    let Some(element) = heap.object(element_id).cloned() else {
        return heap;
    };
    let Some(attrs_id) = attributes_id_of(&element) else {
        return heap;
    };
    let Some(attrs) = heap.object(attrs_id).cloned() else {
        return heap;
    };
    let updated_attrs = attrs.with(name.to_owned(), Value::String(value.to_owned()));
    let heap = heap
        .store_object(attrs_id, updated_attrs)
        .unwrap_or_else(|h| h);
    let mirror_key = match name {
        "id" => Some("id"),
        "class" => Some("className"),
        _other => None,
    };
    if let Some(key) = mirror_key {
        let updated_element = element.with(key.to_owned(), Value::String(value.to_owned()));
        heap.store_object(element_id, updated_element)
            .unwrap_or_else(|h| h)
    } else {
        heap
    }
}

fn string_arg(args: &[Value], idx: usize) -> String {
    match args.get(idx) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => format!("{other}"),
        None => String::new(),
    }
}

/// Build an attributes object (`__attributes`) from a list of pairs.
#[must_use]
pub fn build_attributes_object(pairs: &[(String, String)], heap: Heap) -> (Value, Heap) {
    let map: BTreeMap<String, Value> = pairs
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    let (id, heap) = heap.alloc_object(Object::from_properties(map));
    (Value::Object(id), heap)
}

/// `Element.querySelector(selector)` -- v0.7.8 supports the
/// standard surface most real-world scripts need: comma-separated
/// selector lists; tag / `.class` (multiple) / `#id` / `[attr]` /
/// `[attr="value"]` / `[attr^="value"]` / `[attr$="value"]` /
/// `[attr*="value"]` / `*` as compound parts; whitespace
/// (descendant) and `>` (child) combinators between compounds.
/// Sibling combinators (`+`, `~`), pseudo-classes / pseudo-
/// elements, and `:not()` are deferred.
///
/// # Errors
///
/// Never returns `Err`; no match yields `Value::Null`.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::unnecessary_wraps)]
pub fn query_selector_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let selector = string_arg(&args, 0);
    let list = parse_selector_list(&selector);
    let outcome = object_id_of(&this)
        .and_then(|root_id| find_first_descendant_matching(root_id, &list, &heap))
        .map_or(Outcome::Normal(Value::Null), Outcome::Normal);
    Ok((outcome, heap, fuel))
}

/// `Element.getElementsByTagName(name)` (v0.7.14).  Walks every
/// descendant in depth-first pre-order and returns those whose
/// tag matches `name` (case-insensitive) as a `NodeList`-shaped
/// Object.  The literal `'*'` matches all descendants; empty
/// input returns an empty `NodeList` per spec.  Internally this
/// just rewrites `name` as a selector string and delegates to
/// the v0.7.11 / v0.7.12 / v0.7.13 selector machinery -- a tag
/// name is a valid one-compound selector with no combinators.
///
/// # Errors
///
/// Never returns `Err`; bad inputs yield an empty `NodeList`
/// (`length === 0`).
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn get_elements_by_tag_name_impl(
    args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let tag = string_arg(&args, 0);
    let matches = if tag.is_empty() {
        Vec::new()
    } else {
        object_id_of(&this)
            .map(|root_id| find_all_descendants(root_id, &tag, &heap))
            .unwrap_or_default()
    };
    let (value, heap) = build_node_list(matches, heap);
    Ok((Outcome::Normal(value), heap, fuel))
}

/// `Element.getElementsByClassName(classNames)` (v0.7.14).
/// Walks every descendant in depth-first pre-order and returns
/// those carrying ALL of the (whitespace-separated) classes in
/// `classNames` as a `NodeList`-shaped Object.  Empty / whitespace-
/// only input returns an empty `NodeList` per spec (no implicit
/// match-everything).  Internally we rewrite each class name as
/// a `.foo.bar` selector compound and delegate to the v0.7.11+
/// selector machinery.
///
/// # Errors
///
/// Never returns `Err`; bad inputs yield an empty `NodeList`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn get_elements_by_class_name_impl(
    args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let text = string_arg(&args, 0);
    let matches = class_names_to_selector_string(&text)
        .and_then(|selector| {
            object_id_of(&this).map(|root_id| find_all_descendants(root_id, &selector, &heap))
        })
        .unwrap_or_default();
    let (value, heap) = build_node_list(matches, heap);
    Ok((Outcome::Normal(value), heap, fuel))
}

/// v0.7.14 helper: convert a space-separated class list into a
/// concatenated `.foo.bar` selector string.  Returns `None` for
/// empty / whitespace-only input so callers can short-circuit to
/// an empty result.
fn class_names_to_selector_string(text: &str) -> Option<String> {
    let classes: Vec<&str> = text.split_whitespace().collect();
    if classes.is_empty() {
        None
    } else {
        Some(
            classes
                .iter()
                .fold(String::new(), |acc, c| format!("{acc}.{c}")),
        )
    }
}

/// `Element.matches(selector)` -- v0.7.13.  Returns `true` when
/// `this` itself matches `selector`; does not walk the descendant
/// tree.  Selector grammar identical to `querySelector` /
/// `querySelectorAll` (v0.7.8 / v0.7.11 / v0.7.12).
///
/// # Errors
///
/// Never returns `Err`; bad inputs return `false`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn matches_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let selector = string_arg(&args, 0);
    let list = parse_selector_list(&selector);
    let matched = object_id_of(&this).is_some_and(|id| matches_selector_list(id, &list, &heap));
    Ok((Outcome::Normal(Value::Boolean(matched)), heap, fuel))
}

/// `Element.closest(selector)` -- v0.7.13.  Walks the
/// (`this`, parent, grandparent, ...) chain via the v0.6.8
/// `__parent__` slot and returns the first element matching
/// `selector`, or `Value::Null` if none.  Per spec, `this` itself
/// is the first candidate (closest is inclusive of self).
///
/// # Errors
///
/// Never returns `Err`; no match yields `Value::Null`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn closest_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let selector = string_arg(&args, 0);
    let list = parse_selector_list(&selector);
    let result = object_id_of(&this)
        .and_then(|id| find_closest_matching(id, &list, &heap))
        .map_or(Value::Null, Value::Object);
    Ok((Outcome::Normal(result), heap, fuel))
}

fn find_closest_matching(start_id: ObjectId, list: &SelectorList, heap: &Heap) -> Option<ObjectId> {
    std::iter::successors(Some(start_id), |id| parent_element_id(*id, heap))
        .find(|id| matches_selector_list(*id, list, heap))
}

/// `Element.querySelectorAll(selector)` -- v0.7.11 companion to
/// `querySelector`.  Walks every descendant in depth-first
/// pre-order and returns ALL matches as a NodeList-shaped Object
/// (numeric-key entries + `length`, same shape as `children`),
/// rather than stopping at the first match.  Same selector
/// grammar as v0.7.8 (`tag` / `.class` / `#id` / `[attr...]` /
/// `*` / descendant + child combinators / comma-separated lists).
///
/// # Errors
///
/// Never returns `Err`; no matches yields an empty `NodeList`
/// (`length === 0`).
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn query_selector_all_impl(
    args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let selector = string_arg(&args, 0);
    let matches = object_id_of(&this)
        .map(|root_id| find_all_descendants(root_id, &selector, &heap))
        .unwrap_or_default();
    let (value, heap) = build_node_list(matches, heap);
    Ok((Outcome::Normal(value), heap, fuel))
}

#[derive(Debug, Clone, Default)]
struct SelectorList {
    selectors: Vec<ComplexSelector>,
}

#[derive(Debug, Clone, Default)]
struct ComplexSelector {
    compounds: Vec<CompoundSelector>,
    combinators: Vec<Combinator>,
}

#[derive(Debug, Clone, Copy)]
enum Combinator {
    Descendant,
    Child,
    /// `a + b` -- v0.7.12.  b's immediately-preceding sibling
    /// must match a.
    AdjacentSibling,
    /// `a ~ b` -- v0.7.12.  Any of b's preceding siblings must
    /// match a.
    GeneralSibling,
}

#[derive(Debug, Clone, Default)]
struct CompoundSelector {
    universal: bool,
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attributes: Vec<AttributeMatch>,
    pseudo_classes: Vec<PseudoClass>,
}

#[derive(Debug, Clone)]
enum PseudoClass {
    /// `:first-child` -- this element has no preceding sibling.
    FirstChild,
    /// `:last-child` -- this element has no following sibling.
    LastChild,
    /// `:only-child` -- the element is the sole child of its
    /// parent (both first AND last).
    OnlyChild,
    /// `:not(arg)` (v0.7.15) -- matches an element NOT matching
    /// `arg`.  `arg` is a full `SelectorList` so
    /// `:not(.foo, .bar)` works.  Boxed because `SelectorList`
    /// can transitively contain `PseudoClass::Not` again
    /// (nested negations parse fine via `find_balanced_close_paren`,
    /// matching CSS Selectors Level 4 semantics).
    Not(Box<SelectorList>),
}

#[derive(Debug, Clone)]
struct AttributeMatch {
    name: String,
    operator: AttrOperator,
    value: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum AttrOperator {
    Exists,
    Equals,
    Contains,
    StartsWith,
    EndsWith,
}

fn parse_selector_list(source: &str) -> SelectorList {
    let selectors: Vec<ComplexSelector> = source
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_complex_selector)
        .collect();
    SelectorList { selectors }
}

fn parse_complex_selector(source: &str) -> ComplexSelector {
    let bytes = source.trim().as_bytes();
    let (first_compound, pos) = parse_compound(bytes, 0);
    parse_complex_tail(bytes, pos, vec![first_compound], Vec::new())
}

fn parse_complex_tail(
    bytes: &[u8],
    pos: usize,
    compounds: Vec<CompoundSelector>,
    combinators: Vec<Combinator>,
) -> ComplexSelector {
    let after_ws = skip_whitespace(bytes, pos);
    if after_ws >= bytes.len() {
        ComplexSelector {
            compounds,
            combinators,
        }
    } else if matches!(bytes.get(after_ws), Some(b'>' | b'+' | b'~')) {
        let combinator = match bytes.get(after_ws) {
            Some(b'>') => Combinator::Child,
            Some(b'+') => Combinator::AdjacentSibling,
            Some(b'~') => Combinator::GeneralSibling,
            Some(_) | None => Combinator::Descendant,
        };
        let after_sym = skip_whitespace(bytes, after_ws + 1);
        let (compound, end) = parse_compound(bytes, after_sym);
        let new_compounds: Vec<CompoundSelector> = compounds
            .into_iter()
            .chain(std::iter::once(compound))
            .collect();
        let new_combinators: Vec<Combinator> = combinators
            .into_iter()
            .chain(std::iter::once(combinator))
            .collect();
        parse_complex_tail(bytes, end, new_compounds, new_combinators)
    } else if after_ws > pos {
        let (compound, end) = parse_compound(bytes, after_ws);
        let new_compounds: Vec<CompoundSelector> = compounds
            .into_iter()
            .chain(std::iter::once(compound))
            .collect();
        let new_combinators: Vec<Combinator> = combinators
            .into_iter()
            .chain(std::iter::once(Combinator::Descendant))
            .collect();
        parse_complex_tail(bytes, end, new_compounds, new_combinators)
    } else {
        ComplexSelector {
            compounds,
            combinators,
        }
    }
}

fn parse_compound(bytes: &[u8], start: usize) -> (CompoundSelector, usize) {
    parse_compound_recursive(bytes, start, CompoundSelector::default())
}

#[allow(clippy::too_many_lines)]
fn parse_compound_recursive(
    bytes: &[u8],
    start: usize,
    acc: CompoundSelector,
) -> (CompoundSelector, usize) {
    match bytes.get(start) {
        None | Some(b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'+' | b'~' | b',') => (acc, start),
        Some(b':') => {
            let (parsed, after) = parse_pseudo_class(bytes, start);
            if let Some(pseudo) = parsed {
                let new_pseudos: Vec<PseudoClass> = acc
                    .pseudo_classes
                    .iter()
                    .cloned()
                    .chain(std::iter::once(pseudo))
                    .collect();
                parse_compound_recursive(
                    bytes,
                    after,
                    CompoundSelector {
                        pseudo_classes: new_pseudos,
                        ..acc
                    },
                )
            } else {
                parse_compound_recursive(bytes, after, acc)
            }
        }
        Some(b'*') => parse_compound_recursive(
            bytes,
            start + 1,
            CompoundSelector {
                universal: true,
                ..acc
            },
        ),
        Some(b'.') => {
            let end = scan_ident(bytes, start + 1);
            let name = ident_string(bytes, start + 1, end);
            let new_classes: Vec<String> = acc
                .classes
                .iter()
                .cloned()
                .chain(std::iter::once(name))
                .collect();
            parse_compound_recursive(
                bytes,
                end,
                CompoundSelector {
                    classes: new_classes,
                    ..acc
                },
            )
        }
        Some(b'#') => {
            let end = scan_ident(bytes, start + 1);
            let name = ident_string(bytes, start + 1, end);
            parse_compound_recursive(
                bytes,
                end,
                CompoundSelector {
                    id: Some(name),
                    ..acc
                },
            )
        }
        Some(b'[') => {
            let (attr, end) = parse_attribute_match(bytes, start + 1);
            let new_attrs: Vec<AttributeMatch> = acc
                .attributes
                .iter()
                .cloned()
                .chain(std::iter::once(attr))
                .collect();
            parse_compound_recursive(
                bytes,
                end,
                CompoundSelector {
                    attributes: new_attrs,
                    ..acc
                },
            )
        }
        Some(_) => {
            let end = scan_ident(bytes, start);
            if end <= start {
                (acc, start + 1)
            } else {
                let name = ident_string(bytes, start, end).to_ascii_lowercase();
                if acc.tag.is_none() && !acc.universal && acc.id.is_none() {
                    parse_compound_recursive(
                        bytes,
                        end,
                        CompoundSelector {
                            tag: Some(name),
                            ..acc
                        },
                    )
                } else {
                    (acc, start)
                }
            }
        }
    }
}

/// v0.7.13 / v0.7.15 helper: parse a pseudo-class starting at
/// the leading `:`.  Returns `(Some(pseudo), after_pos)` when the
/// name is recognised, `(None, after_pos)` for unknown names
/// (forward-compat for `:nth-child(n)` etc.).  Handles both the
/// bare form `:first-child` and the `:not(arg)` paren form via
/// [`find_balanced_close_paren`] -- the inner argument is parsed
/// recursively as a `SelectorList` so `:not(.a, .b)` works.
fn parse_pseudo_class(bytes: &[u8], start: usize) -> (Option<PseudoClass>, usize) {
    let ident_end = scan_ident(bytes, start + 1);
    let name = ident_string(bytes, start + 1, ident_end).to_ascii_lowercase();
    if bytes.get(ident_end) == Some(&b'(') {
        let arg_start = ident_end + 1;
        let close = find_balanced_close_paren(bytes, arg_start, 1);
        let arg_text = ident_string(bytes, arg_start, close);
        let after_close = if bytes.get(close) == Some(&b')') {
            close + 1
        } else {
            close
        };
        let parsed = match name.as_str() {
            "not" => Some(PseudoClass::Not(Box::new(parse_selector_list(&arg_text)))),
            _other => None,
        };
        (parsed, after_close)
    } else {
        let parsed = match name.as_str() {
            "first-child" => Some(PseudoClass::FirstChild),
            "last-child" => Some(PseudoClass::LastChild),
            "only-child" => Some(PseudoClass::OnlyChild),
            _other => None,
        };
        (parsed, ident_end)
    }
}

/// v0.7.15 helper: walk `bytes` from `start` (just past an opening
/// `(`) until the matching close `)` at nesting depth 1.  Supports
/// arbitrary paren nesting so `:not(:not(.foo))` parses correctly.
/// Returns the index of the matching `)`, or `bytes.len()` when
/// the input is unbalanced (treated as graceful EOF).
fn find_balanced_close_paren(bytes: &[u8], start: usize, depth: u32) -> usize {
    match bytes.get(start) {
        None => bytes.len(),
        Some(b'(') => find_balanced_close_paren(bytes, start + 1, depth + 1),
        Some(b')') if depth == 1 => start,
        Some(b')') => find_balanced_close_paren(bytes, start + 1, depth - 1),
        Some(_) => find_balanced_close_paren(bytes, start + 1, depth),
    }
}

fn parse_attribute_match(bytes: &[u8], start: usize) -> (AttributeMatch, usize) {
    let name_start = skip_whitespace(bytes, start);
    let name_end = scan_ident(bytes, name_start);
    let name = ident_string(bytes, name_start, name_end);
    let after_name = skip_whitespace(bytes, name_end);
    match bytes.get(after_name) {
        Some(b']') => (
            AttributeMatch {
                name,
                operator: AttrOperator::Exists,
                value: None,
            },
            after_name + 1,
        ),
        Some(b'=') => {
            parse_attribute_with_operator(bytes, after_name + 1, name, AttrOperator::Equals)
        }
        Some(b'*' | b'^' | b'$') => {
            let op = match bytes.get(after_name) {
                Some(b'*') => AttrOperator::Contains,
                Some(b'^') => AttrOperator::StartsWith,
                Some(b'$') => AttrOperator::EndsWith,
                Some(b'=' | b']' | _) | None => AttrOperator::Equals,
            };
            let after_op = match bytes.get(after_name + 1) {
                Some(b'=') => after_name + 2,
                Some(_) | None => after_name + 1,
            };
            parse_attribute_with_operator(bytes, after_op, name, op)
        }
        None | Some(_) => (
            AttributeMatch {
                name,
                operator: AttrOperator::Exists,
                value: None,
            },
            after_name,
        ),
    }
}

fn parse_attribute_with_operator(
    bytes: &[u8],
    start: usize,
    name: String,
    operator: AttrOperator,
) -> (AttributeMatch, usize) {
    let after_ws = skip_whitespace(bytes, start);
    let (value, after_value) = parse_attribute_value(bytes, after_ws);
    let after_close = match bytes.get(skip_whitespace(bytes, after_value)) {
        Some(b']') => skip_whitespace(bytes, after_value) + 1,
        Some(_) | None => after_value,
    };
    (
        AttributeMatch {
            name,
            operator,
            value: Some(value),
        },
        after_close,
    )
}

fn parse_attribute_value(bytes: &[u8], start: usize) -> (String, usize) {
    match bytes.get(start) {
        Some(b'"') => parse_quoted(bytes, start + 1, b'"'),
        Some(b'\'') => parse_quoted(bytes, start + 1, b'\''),
        Some(_) => {
            let end = scan_ident(bytes, start);
            (ident_string(bytes, start, end), end)
        }
        None => (String::new(), start),
    }
}

fn parse_quoted(bytes: &[u8], start: usize, quote: u8) -> (String, usize) {
    let end = (start..bytes.len())
        .find(|i| bytes.get(*i) == Some(&quote))
        .unwrap_or(bytes.len());
    let value = ident_string(bytes, start, end);
    let after = if end < bytes.len() { end + 1 } else { end };
    (value, after)
}

fn skip_whitespace(bytes: &[u8], start: usize) -> usize {
    (start..bytes.len())
        .find(|i| !matches!(bytes.get(*i), Some(b' ' | b'\t' | b'\n' | b'\r')))
        .unwrap_or(bytes.len())
}

fn scan_ident(bytes: &[u8], start: usize) -> usize {
    bytes
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, b)| !is_ident_byte(**b))
        .map_or(bytes.len(), |(i, _)| i)
}

fn ident_string(bytes: &[u8], start: usize, end: usize) -> String {
    bytes
        .get(start..end)
        .and_then(|s| std::str::from_utf8(s).ok())
        .unwrap_or("")
        .to_owned()
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn find_first_descendant_matching(
    root: ObjectId,
    list: &SelectorList,
    heap: &Heap,
) -> Option<Value> {
    let children = element_children(root, heap);
    children.iter().find_map(|child_id| {
        if matches_selector_list(*child_id, list, heap) {
            Some(Value::Object(*child_id))
        } else {
            find_first_descendant_matching(*child_id, list, heap)
        }
    })
}

fn find_all_descendants_matching(
    root: ObjectId,
    list: &SelectorList,
    heap: &Heap,
) -> Vec<ObjectId> {
    let children = element_children(root, heap);
    children
        .iter()
        .flat_map(|child_id| {
            let matched_self = matches_selector_list(*child_id, list, heap)
                .then_some(*child_id)
                .into_iter();
            matched_self.chain(find_all_descendants_matching(*child_id, list, heap))
        })
        .collect()
}

fn matches_selector_list(element_id: ObjectId, list: &SelectorList, heap: &Heap) -> bool {
    list.selectors
        .iter()
        .any(|sel| matches_complex_selector(element_id, sel, heap))
}

fn matches_complex_selector(candidate_id: ObjectId, sel: &ComplexSelector, heap: &Heap) -> bool {
    let Some(last_idx) = sel.compounds.len().checked_sub(1) else {
        return false;
    };
    let Some(last_compound) = sel.compounds.get(last_idx) else {
        return false;
    };
    if !matches_compound(candidate_id, last_compound, heap) {
        return false;
    }
    (0..last_idx)
        .rev()
        .try_fold(candidate_id, |current, i| {
            let combinator = sel.combinators.get(i)?;
            let target_compound = sel.compounds.get(i)?;
            match combinator {
                Combinator::Child => {
                    let parent_id = parent_element_id(current, heap)?;
                    matches_compound(parent_id, target_compound, heap).then_some(parent_id)
                }
                Combinator::Descendant => walk_ancestors_find_match(current, target_compound, heap),
                Combinator::AdjacentSibling => {
                    let prev_id = previous_sibling_id(current, heap)?;
                    matches_compound(prev_id, target_compound, heap).then_some(prev_id)
                }
                Combinator::GeneralSibling => {
                    walk_previous_siblings_find_match(current, target_compound, heap)
                }
            }
        })
        .is_some()
}

fn matches_compound(node_id: ObjectId, compound: &CompoundSelector, heap: &Heap) -> bool {
    let Some(object) = heap.object(node_id) else {
        return false;
    };
    let pseudos_ok = compound
        .pseudo_classes
        .iter()
        .all(|p| matches_pseudo_class(node_id, p, heap));
    if !pseudos_ok {
        return false;
    }
    let tag_ok = compound
        .tag
        .as_ref()
        .is_none_or(|want| string_property(object, "tagName").eq_ignore_ascii_case(want));
    let id_ok = compound
        .id
        .as_ref()
        .is_none_or(|want| string_property(object, "id") == *want);
    let class_value = string_property(object, "className");
    let class_ok = compound.classes.iter().all(|want| {
        class_value
            .split_ascii_whitespace()
            .any(|present| present == want)
    });
    let attrs_ok = compound
        .attributes
        .iter()
        .all(|am| matches_attribute(object, am, heap));
    tag_ok && id_ok && class_ok && attrs_ok
}

fn matches_attribute(element: &Object, am: &AttributeMatch, heap: &Heap) -> bool {
    let Some(attrs_id) = attributes_id_of(element) else {
        return false;
    };
    let Some(attrs) = heap.object(attrs_id) else {
        return false;
    };
    let stored = attrs.get(&am.name).and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::Object(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => None,
    });
    match (am.operator, stored, &am.value) {
        (AttrOperator::Exists, Some(_), _) => true,
        (AttrOperator::Equals, Some(actual), Some(want)) => actual == *want,
        (AttrOperator::Contains, Some(actual), Some(want)) => actual.contains(want.as_str()),
        (AttrOperator::StartsWith, Some(actual), Some(want)) => actual.starts_with(want.as_str()),
        (AttrOperator::EndsWith, Some(actual), Some(want)) => actual.ends_with(want.as_str()),
        (AttrOperator::Exists, None, _)
        | (
            AttrOperator::Equals
            | AttrOperator::Contains
            | AttrOperator::StartsWith
            | AttrOperator::EndsWith,
            None | Some(_),
            None | Some(_),
        ) => false,
    }
}

fn parent_element_id(node_id: ObjectId, heap: &Heap) -> Option<ObjectId> {
    let obj = heap.object(node_id)?;
    obj.get("__parent__").and_then(|v| match v {
        Value::Object(id) => Some(*id),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => None,
    })
}

fn walk_ancestors_find_match(
    start_id: ObjectId,
    compound: &CompoundSelector,
    heap: &Heap,
) -> Option<ObjectId> {
    std::iter::successors(parent_element_id(start_id, heap), |id| {
        parent_element_id(*id, heap)
    })
    .find(|id| matches_compound(*id, compound, heap))
}

/// v0.7.12 helper: the element immediately before `node_id` in
/// its parent's children-array Object, or `None` for the first
/// child / detached node.
fn previous_sibling_id(node_id: ObjectId, heap: &Heap) -> Option<ObjectId> {
    let parent_id = parent_element_id(node_id, heap)?;
    let parent = heap.object(parent_id)?;
    let children_id = parent.get("children").and_then(object_id_of)?;
    let children = heap.object(children_id)?;
    let length = array_length(children);
    let position = (0..length)
        .find(|i| children.get(&format!("{i}")).and_then(object_id_of) == Some(node_id))?;
    position
        .checked_sub(1)
        .and_then(|prev_idx| children.get(&format!("{prev_idx}")).and_then(object_id_of))
}

/// v0.7.12 helper: walk back through `node_id`'s preceding
/// siblings in document order (closest first) and return the
/// first one matching `compound`.
fn walk_previous_siblings_find_match(
    start_id: ObjectId,
    compound: &CompoundSelector,
    heap: &Heap,
) -> Option<ObjectId> {
    std::iter::successors(previous_sibling_id(start_id, heap), |id| {
        previous_sibling_id(*id, heap)
    })
    .find(|id| matches_compound(*id, compound, heap))
}

/// v0.7.13 helper: the element immediately after `node_id` in
/// its parent's children-array Object, or `None` for the last
/// child / detached node.  Used by `:last-child` /
/// `:only-child` pseudo-class checks.
fn next_sibling_id(node_id: ObjectId, heap: &Heap) -> Option<ObjectId> {
    let parent_id = parent_element_id(node_id, heap)?;
    let parent = heap.object(parent_id)?;
    let children_id = parent.get("children").and_then(object_id_of)?;
    let children = heap.object(children_id)?;
    let length = array_length(children);
    let position = (0..length)
        .find(|i| children.get(&format!("{i}")).and_then(object_id_of) == Some(node_id))?;
    children
        .get(&format!("{}", position + 1))
        .and_then(object_id_of)
}

fn matches_pseudo_class(node_id: ObjectId, pseudo: &PseudoClass, heap: &Heap) -> bool {
    // Structural pseudo-classes require an actual parent element
    // -- detached / root nodes match none.  `:not(...)` doesn't
    // need the guard since negation should apply uniformly.
    match pseudo {
        PseudoClass::FirstChild => {
            has_parent(node_id, heap) && previous_sibling_id(node_id, heap).is_none()
        }
        PseudoClass::LastChild => {
            has_parent(node_id, heap) && next_sibling_id(node_id, heap).is_none()
        }
        PseudoClass::OnlyChild => {
            has_parent(node_id, heap)
                && previous_sibling_id(node_id, heap).is_none()
                && next_sibling_id(node_id, heap).is_none()
        }
        PseudoClass::Not(list) => !matches_selector_list(node_id, list, heap),
    }
}

fn has_parent(node_id: ObjectId, heap: &Heap) -> bool {
    parent_element_id(node_id, heap).is_some()
}

fn element_children(node_id: ObjectId, heap: &Heap) -> Vec<ObjectId> {
    let Some(object) = heap.object(node_id) else {
        return Vec::new();
    };
    let Some(children_id) = object.get("children").and_then(object_id_of) else {
        return Vec::new();
    };
    let Some(children) = heap.object(children_id) else {
        return Vec::new();
    };
    let length = array_length(children);
    (0..length)
        .filter_map(|i| children.get(&format!("{i}")).and_then(object_id_of))
        .collect()
}

fn array_length(array: &Object) -> u32 {
    match array.get("length") {
        Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let len = *n as u32;
            len
        }
        Some(_) | None => 0,
    }
}

fn string_property(object: &Object, key: &str) -> String {
    match object.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(_) | None => String::new(),
    }
}

/// Public helper used by `document.querySelector` to perform the search
/// from the document root.
#[must_use]
pub fn find_first_descendant(root: ObjectId, pattern_source: &str, heap: &Heap) -> Option<Value> {
    let list = parse_selector_list(pattern_source);
    find_first_descendant_matching(root, &list, heap)
}

/// Public helper used by `document.querySelectorAll` to collect
/// every descendant of `root` matching `pattern_source`, in
/// depth-first pre-order.  Returns the raw `ObjectId` list so
/// the caller can wrap them in whatever shape is appropriate
/// (`NodeList` via [`build_node_list`], a `Vec<Value>`, etc.).
#[must_use]
pub fn find_all_descendants(root: ObjectId, pattern_source: &str, heap: &Heap) -> Vec<ObjectId> {
    let list = parse_selector_list(pattern_source);
    find_all_descendants_matching(root, &list, heap)
}

/// Public helper that wraps `matches` as a `NodeList`-shaped
/// Object -- numeric-key entries plus `length`, same shape that
/// `Element.children` uses so script-side iteration patterns
/// (`for (let i = 0; i < list.length; i++)` and `list[i]`) work
/// uniformly.
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn build_node_list(matches: Vec<ObjectId>, heap: Heap) -> (Value, Heap) {
    let length = u32::try_from(matches.len()).unwrap_or(u32::MAX);
    let pairs: BTreeMap<String, Value> = matches
        .iter()
        .enumerate()
        .map(|(i, id)| (format!("{i}"), Value::Object(*id)))
        .chain(std::iter::once((
            "length".to_owned(),
            Value::Number(f64::from(length)),
        )))
        .collect();
    let (id, heap) = heap.alloc_object(Object::from_properties(pairs));
    (Value::Object(id), heap)
}

/// Public helper for `document.getElementById`.
#[must_use]
pub fn find_by_id(root: ObjectId, id: &str, heap: &Heap) -> Option<Value> {
    let compound = CompoundSelector {
        id: Some(id.to_owned()),
        ..CompoundSelector::default()
    };
    let complex = ComplexSelector {
        compounds: vec![compound],
        combinators: Vec::new(),
    };
    let list = SelectorList {
        selectors: vec![complex],
    };
    find_first_descendant_matching(root, &list, heap)
}

/// `Element.appendChild(child)` (v0.6.1): append `child` to
/// `this.children`, updating `length`.  Returns `child` per spec
/// so the caller can chain.  No-ops when `this` or `child` aren't
/// Object values, or when the children-array slot is missing /
/// non-object.
///
/// Mutation flow: clone the children array Object, write the new
/// numeric-key entry, write `length`, then `store_object` both the
/// children Object and the parent (the parent's `children` slot
/// already points at the same children-array `ObjectId`, so the
/// parent update is implicit -- only the children Object's
/// in-place mutation matters).  `extract_document` walks
/// `children` numerically in the next back-prop pass, so the new
/// child reaches layout / paint automatically.
///
/// # Errors
///
/// Never returns `Err`; bad inputs yield `Value::Undefined`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn append_child_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let child = args.first().cloned().unwrap_or(Value::Undefined);
    let outcome_heap = append_child_to_parent(&this, &child, heap);
    Ok((Outcome::Normal(child), outcome_heap, fuel))
}

fn append_child_to_parent(this: &Value, child: &Value, heap: Heap) -> Heap {
    let Some(parent_id) = object_id_of(this) else {
        return heap;
    };
    if let Some((children_id, children_obj)) = children_object_of(this, &heap) {
        let length = array_length(&children_obj);
        let updated_children = children_obj
            .with(format!("{length}"), child.clone())
            .with("length".to_owned(), Value::Number(f64::from(length + 1)));
        let heap = heap
            .store_object(children_id, updated_children)
            .unwrap_or_else(|h| h);
        set_parent_backref(child, parent_id, heap)
    } else {
        heap
    }
}

/// v0.6.8 helper: set `child_value.__parent__` to
/// `Value::Object(parent_id)`.  No-op when `child_value` isn't an
/// Object or its heap slot is missing.  Called by every structural
/// mutator and by `document::build_element` / `document::clone_element`
/// to wire up parent backrefs.
#[must_use]
pub fn set_parent_backref(child_value: &Value, parent_id: ObjectId, heap: Heap) -> Heap {
    let Some(child_id) = object_id_of(child_value) else {
        return heap;
    };
    let Some(child_obj) = heap.object(child_id).cloned() else {
        return heap;
    };
    let updated = child_obj.with("__parent__".to_owned(), Value::Object(parent_id));
    heap.store_object(child_id, updated).unwrap_or_else(|h| h)
}

/// v0.6.8 helper: clear `child_value.__parent__` to `Value::Null`.
/// No-op when `child_value` isn't an Object or its heap slot is
/// missing.  Called when detaching a child from its parent
/// (`removeChild`, `replaceChild`'s old-child slot, `remove`,
/// `innerHTML` setter's discarded old children).
#[must_use]
pub fn clear_parent_backref(child_value: &Value, heap: Heap) -> Heap {
    let Some(child_id) = object_id_of(child_value) else {
        return heap;
    };
    let Some(child_obj) = heap.object(child_id).cloned() else {
        return heap;
    };
    let updated = child_obj.with("__parent__".to_owned(), Value::Null);
    heap.store_object(child_id, updated).unwrap_or_else(|h| h)
}

fn children_object_of(this: &Value, heap: &Heap) -> Option<(ObjectId, Object)> {
    let parent_id = object_id_of(this)?;
    let parent = heap.object(parent_id)?;
    let children_id = parent.get("children").and_then(|v| match v {
        Value::Object(id) => Some(*id),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => None,
    })?;
    let children_obj = heap.object(children_id)?.clone();
    Some((children_id, children_obj))
}

/// `Element.removeChild(child)` (v0.6.2): remove `child` from
/// `this.children` and return it.  Remaining children shift down
/// to fill the freed slot; `length` decrements.  No-ops (and still
/// returns the requested `child`) when `this` or `child` aren't
/// Object values, the children-array slot is missing / non-object,
/// or `child` isn't currently a child of `this` (the DOM spec
/// throws `NotFoundError` here; this scoped impl no-ops to stay
/// inside the always-`Ok` `NativeFn` contract).
///
/// # Errors
///
/// Never returns `Err`; bad inputs no-op.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn remove_child_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let child = args.first().cloned().unwrap_or(Value::Undefined);
    let outcome_heap = remove_child_from_parent(&this, &child, heap);
    Ok((Outcome::Normal(child), outcome_heap, fuel))
}

fn remove_child_from_parent(this: &Value, child: &Value, heap: Heap) -> Heap {
    let Some((children_id, children_obj)) = children_object_of(this, &heap) else {
        return heap;
    };
    let Some(child_id) = object_id_of(child) else {
        return heap;
    };
    let length = array_length(&children_obj);
    let Some(removal_index) = (0..length)
        .find(|i| children_obj.get(&format!("{i}")).and_then(object_id_of) == Some(child_id))
    else {
        return heap;
    };
    let new_length = length.saturating_sub(1);
    let pairs: BTreeMap<String, Value> = (0..length)
        .filter(|i| *i != removal_index)
        .enumerate()
        .map(|(new_idx, old_idx)| {
            let value = children_obj
                .get(&format!("{old_idx}"))
                .cloned()
                .unwrap_or(Value::Null);
            (format!("{new_idx}"), value)
        })
        .chain(std::iter::once((
            "length".to_owned(),
            Value::Number(f64::from(new_length)),
        )))
        .collect();
    let new_children = Object::from_properties(pairs);
    let heap = heap
        .store_object(children_id, new_children)
        .unwrap_or_else(|h| h);
    clear_parent_backref(child, heap)
}

/// `Element.insertBefore(newNode, referenceNode)` (v0.6.2): insert
/// `newNode` into `this.children` immediately before
/// `referenceNode`, returning `newNode`.  Existing children at and
/// after `referenceNode`'s slot shift up; `length` increments.  If
/// `referenceNode` is `null` / `undefined` / any non-Object value
/// the call falls through to `appendChild`-like end-of-list
/// behaviour (matching the DOM spec's `null` case).  No-ops if
/// `this` isn't an Object, the children-array slot is missing /
/// non-object, or `referenceNode` is a real Object but isn't
/// actually a child of `this` (the DOM spec throws
/// `NotFoundError`; this scoped impl no-ops).
///
/// # Errors
///
/// Never returns `Err`; bad inputs yield `Value::Undefined`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn insert_before_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let new_node = args.first().cloned().unwrap_or(Value::Undefined);
    let ref_node = args.get(1).cloned().unwrap_or(Value::Null);
    let outcome_heap = insert_before_into_parent(&this, &new_node, &ref_node, heap);
    Ok((Outcome::Normal(new_node), outcome_heap, fuel))
}

fn insert_before_into_parent(this: &Value, new_node: &Value, ref_node: &Value, heap: Heap) -> Heap {
    let ref_id_opt = object_id_of(ref_node);
    if ref_id_opt.is_none() {
        append_child_to_parent(this, new_node, heap)
    } else {
        let Some(parent_id) = object_id_of(this) else {
            return heap;
        };
        let Some((children_id, children_obj)) = children_object_of(this, &heap) else {
            return heap;
        };
        let Some(ref_id) = ref_id_opt else {
            return heap;
        };
        let length = array_length(&children_obj);
        let Some(insert_index) = (0..length)
            .find(|i| children_obj.get(&format!("{i}")).and_then(object_id_of) == Some(ref_id))
        else {
            return heap;
        };
        let new_length = length + 1;
        let pairs: BTreeMap<String, Value> = (0..new_length)
            .map(|new_idx| {
                let value = match new_idx.cmp(&insert_index) {
                    std::cmp::Ordering::Less => children_obj
                        .get(&format!("{new_idx}"))
                        .cloned()
                        .unwrap_or(Value::Null),
                    std::cmp::Ordering::Equal => new_node.clone(),
                    std::cmp::Ordering::Greater => {
                        let old_idx = new_idx - 1;
                        children_obj
                            .get(&format!("{old_idx}"))
                            .cloned()
                            .unwrap_or(Value::Null)
                    }
                };
                (format!("{new_idx}"), value)
            })
            .chain(std::iter::once((
                "length".to_owned(),
                Value::Number(f64::from(new_length)),
            )))
            .collect();
        let new_children = Object::from_properties(pairs);
        let heap = heap
            .store_object(children_id, new_children)
            .unwrap_or_else(|h| h);
        set_parent_backref(new_node, parent_id, heap)
    }
}

/// `Element.replaceChild(newChild, oldChild)` (v0.6.7): swap
/// `oldChild` for `newChild` in `this.children`.  Length is
/// preserved (one slot in, one slot out); the surrounding
/// children stay at their indexes.  Returns `oldChild` per spec.
/// No-ops (and still returns the requested `oldChild`) when
/// `this` isn't an Object, the children-array slot is missing /
/// non-object, `oldChild` isn't an Object, or `oldChild` isn't
/// currently a child of `this` (the DOM spec throws
/// `NotFoundError`; this scoped impl no-ops).
///
/// # Errors
///
/// Never returns `Err`; bad inputs no-op.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn replace_child_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let new_child = args.first().cloned().unwrap_or(Value::Undefined);
    let old_child = args.get(1).cloned().unwrap_or(Value::Undefined);
    let outcome_heap = replace_child_in_parent(&this, &new_child, &old_child, heap);
    Ok((Outcome::Normal(old_child), outcome_heap, fuel))
}

fn replace_child_in_parent(this: &Value, new_child: &Value, old_child: &Value, heap: Heap) -> Heap {
    let Some(parent_id) = object_id_of(this) else {
        return heap;
    };
    let Some((children_id, children_obj)) = children_object_of(this, &heap) else {
        return heap;
    };
    let Some(old_id) = object_id_of(old_child) else {
        return heap;
    };
    let length = array_length(&children_obj);
    let Some(replace_index) = (0..length)
        .find(|i| children_obj.get(&format!("{i}")).and_then(object_id_of) == Some(old_id))
    else {
        return heap;
    };
    let updated_children = children_obj.with(format!("{replace_index}"), new_child.clone());
    let heap = heap
        .store_object(children_id, updated_children)
        .unwrap_or_else(|h| h);
    let heap = set_parent_backref(new_child, parent_id, heap);
    clear_parent_backref(old_child, heap)
}

/// `Element.remove()` (v0.6.8): detach `this` from its parent.
/// Reads `this.__parent__`; if it's an Object, calls the same
/// `remove_child_from_parent` helper that backs `removeChild`,
/// which both excises `this` from the parent's children-array
/// Object and clears `this.__parent__`.  No-op when `this` has no
/// parent (already detached, or it's the document root).
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn remove_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let parent_value = parent_value_of(&this, &heap);
    let outcome_heap = match parent_value {
        Value::Object(_) => remove_child_from_parent(&parent_value, &this, heap),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => heap,
    };
    Ok((Outcome::Normal(Value::Undefined), outcome_heap, fuel))
}

fn parent_value_of(this: &Value, heap: &Heap) -> Value {
    object_id_of(this)
        .and_then(|id| heap.object(id))
        .and_then(|obj| obj.get("__parent__").cloned())
        .unwrap_or(Value::Null)
}

/// v0.6.8 getter for `parentElement` / `parentNode` accessors:
/// returns `this.__parent__` (an `Object` value when attached,
/// `Null` when detached or root).  Same implementation for both
/// accessors -- we don't yet distinguish elements from other node
/// types, so `parentElement` and `parentNode` produce identical
/// results.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn parent_getter_impl(_args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let parent = parent_value_of(&this, &heap);
    Ok((Outcome::Normal(parent), heap, fuel))
}

/// v0.6.9 getter for `firstElementChild`: `this.children[0]` or
/// `Value::Null` if `this` has no children.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn first_element_child_getter_impl(
    _args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let value = own_children_view(&this, &heap)
        .filter(|(_, length)| *length > 0)
        .and_then(|(children, _)| children.get("0").cloned())
        .unwrap_or(Value::Null);
    Ok((Outcome::Normal(value), heap, fuel))
}

/// v0.6.9 getter for `lastElementChild`: `this.children[length-1]`
/// or `Value::Null` if `this` has no children.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn last_element_child_getter_impl(
    _args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let value = own_children_view(&this, &heap)
        .filter(|(_, length)| *length > 0)
        .and_then(|(children, length)| children.get(&format!("{}", length - 1)).cloned())
        .unwrap_or(Value::Null);
    Ok((Outcome::Normal(value), heap, fuel))
}

/// v0.6.9 getter for `previousElementSibling`: looks `this` up by
/// id in its parent's children-array Object, returns the entry at
/// `this_index - 1` if it exists, else `Value::Null`.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn previous_element_sibling_getter_impl(
    _args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let value = parent_children_view(&this, &heap)
        .filter(|(_, _, this_index)| *this_index > 0)
        .and_then(|(children, _, this_index)| children.get(&format!("{}", this_index - 1)).cloned())
        .unwrap_or(Value::Null);
    Ok((Outcome::Normal(value), heap, fuel))
}

/// v0.6.9 getter for `nextElementSibling`: looks `this` up by id
/// in its parent's children-array Object, returns the entry at
/// `this_index + 1` if it exists, else `Value::Null`.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn next_element_sibling_getter_impl(
    _args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let value = parent_children_view(&this, &heap)
        .filter(|(_, length, this_index)| this_index + 1 < *length)
        .and_then(|(children, _, this_index)| children.get(&format!("{}", this_index + 1)).cloned())
        .unwrap_or(Value::Null);
    Ok((Outcome::Normal(value), heap, fuel))
}

fn own_children_view<'h>(this: &Value, heap: &'h Heap) -> Option<(&'h Object, u32)> {
    let this_id = object_id_of(this)?;
    let this_obj = heap.object(this_id)?;
    let children_id = this_obj.get("children").and_then(object_id_of)?;
    let children = heap.object(children_id)?;
    let length = array_length(children);
    Some((children, length))
}

fn parent_children_view<'h>(this: &Value, heap: &'h Heap) -> Option<(&'h Object, u32, u32)> {
    let this_id = object_id_of(this)?;
    let this_obj = heap.object(this_id)?;
    let parent_id = this_obj.get("__parent__").and_then(object_id_of)?;
    let parent = heap.object(parent_id)?;
    let children_id = parent.get("children").and_then(object_id_of)?;
    let children = heap.object(children_id)?;
    let length = array_length(children);
    let this_index = (0..length)
        .find(|i| children.get(&format!("{i}")).and_then(object_id_of) == Some(this_id))?;
    Some((children, length, this_index))
}

/// `Element.classList.add(token)` (v0.6.3): if `token` isn't
/// already among the parent element's class tokens, append it and
/// write the rejoined token list back through `setAttribute`-style
/// mirroring (so both `className` and `__attributes.class` stay in
/// sync; `getAttribute('class')` keeps working).  No-ops if `this`
/// isn't a `classList` Object or its `__element__` backref is
/// missing / non-Object.
///
/// # Errors
///
/// Never returns `Err`; bad inputs no-op.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn class_list_add_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let token = string_arg(&args, 0);
    let new_heap = add_class_token(&this, &token, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

/// `Element.classList.remove(token)` (v0.6.3): drop every
/// occurrence of `token` from the parent element's class list and
/// write the rejoined token list back through `setAttribute`-style
/// mirroring.  No-ops if `this` isn't a `classList` Object.
///
/// # Errors
///
/// Never returns `Err`; bad inputs no-op.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn class_list_remove_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let token = string_arg(&args, 0);
    let new_heap = remove_class_token(&this, &token, heap);
    Ok((Outcome::Normal(Value::Undefined), new_heap, fuel))
}

/// `Element.classList.contains(token)` (v0.6.3): return `true` iff
/// `token` is among the parent element's whitespace-separated class
/// tokens.  Returns `false` if `this` isn't a `classList` Object.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn class_list_contains_impl(
    args: Vec<Value>,
    this: Value,
    heap: Heap,
    fuel: Fuel,
) -> EvalResult {
    let token = string_arg(&args, 0);
    let present = element_of_class_list(&this, &heap)
        .is_some_and(|(_, element)| class_list_tokens(&element).iter().any(|t| t == &token));
    Ok((Outcome::Normal(Value::Boolean(present)), heap, fuel))
}

/// `Element.classList.toggle(token)` (v0.6.3): remove `token` if it
/// was already present, otherwise add it; return the new
/// presence-state (matches MDN: `true` after add, `false` after
/// remove).  No-ops (returning `false`) if `this` isn't a
/// `classList` Object.
///
/// # Errors
///
/// Never returns `Err`.
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
pub fn class_list_toggle_impl(args: Vec<Value>, this: Value, heap: Heap, fuel: Fuel) -> EvalResult {
    let token = string_arg(&args, 0);
    let (present, new_heap) = toggle_class_token(&this, &token, heap);
    Ok((Outcome::Normal(Value::Boolean(present)), new_heap, fuel))
}

fn element_of_class_list(this: &Value, heap: &Heap) -> Option<(ObjectId, Object)> {
    let list_id = object_id_of(this)?;
    let list = heap.object(list_id)?;
    let element_id = list.get("__element__").and_then(|v| match v {
        Value::Object(id) => Some(*id),
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Function(_)
        | Value::Native(_)
        | Value::Promise(_) => None,
    })?;
    let element = heap.object(element_id)?.clone();
    Some((element_id, element))
}

fn class_list_tokens(element: &Object) -> Vec<String> {
    match element.get("className") {
        Some(Value::String(s)) => s.split_ascii_whitespace().map(str::to_owned).collect(),
        Some(_) | None => Vec::new(),
    }
}

fn add_class_token(this: &Value, token: &str, heap: Heap) -> Heap {
    let Some((element_id, element)) = element_of_class_list(this, &heap) else {
        return heap;
    };
    let tokens = class_list_tokens(&element);
    let new_tokens: Vec<String> = if tokens.iter().any(|t| t == token) {
        tokens
    } else {
        tokens
            .into_iter()
            .chain(std::iter::once(token.to_owned()))
            .collect()
    };
    let new_class = new_tokens.join(" ");
    let element_value = Value::Object(element_id);
    write_attribute(&element_value, "class", &new_class, heap)
}

fn remove_class_token(this: &Value, token: &str, heap: Heap) -> Heap {
    let Some((element_id, element)) = element_of_class_list(this, &heap) else {
        return heap;
    };
    let tokens = class_list_tokens(&element);
    let new_tokens: Vec<String> = tokens.into_iter().filter(|t| t != token).collect();
    let new_class = new_tokens.join(" ");
    let element_value = Value::Object(element_id);
    write_attribute(&element_value, "class", &new_class, heap)
}

fn toggle_class_token(this: &Value, token: &str, heap: Heap) -> (bool, Heap) {
    let Some((element_id, element)) = element_of_class_list(this, &heap) else {
        return (false, heap);
    };
    let tokens = class_list_tokens(&element);
    let was_present = tokens.iter().any(|t| t == token);
    let new_tokens: Vec<String> = if was_present {
        tokens.into_iter().filter(|t| t != token).collect()
    } else {
        tokens
            .into_iter()
            .chain(std::iter::once(token.to_owned()))
            .collect()
    };
    let new_class = new_tokens.join(" ");
    let element_value = Value::Object(element_id);
    let new_heap = write_attribute(&element_value, "class", &new_class, heap);
    (!was_present, new_heap)
}

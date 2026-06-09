//! Back-propagation: walk the post-script JS-side DOM tree and
//! reconstruct a [`dom_cat::Document`] so callers can re-run layout +
//! paint against the mutated state.
//!
//! v0.2 limitations:
//!
//! - The JS-side representation flattens descendant text into a single
//!   per-element `textContent` string and the `children` array is
//!   element-only.  When an element has non-empty `textContent` and
//!   no element children, [`extract_document`] synthesizes a single
//!   [`dom_cat::Node::Text`] child carrying that string.  Otherwise
//!   text is dropped.  Original text-node positions are not
//!   recoverable.
//! - Comments are not represented JS-side, so they don't survive a
//!   round-trip.
//! - Attributes come back in `BTreeMap` (lexicographic) order, not
//!   source order, because boa-cat `Object` properties are
//!   `BTreeMap<String, Value>`.

use boa_cat::Value;
use boa_cat::heap::Heap;
use boa_cat::value::{Object, ObjectId};
use dom_cat::{Arena, CommentData, Document, DocumentData, ElementData, Node, NodeId, TextData};

/// Walk the JS-side document tree starting at `document_value` and
/// produce a fresh [`dom_cat::Document`] reflecting the current state
/// of the boa-cat `heap`.  Returns `None` if `document_value` does not
/// point to a document object built by this crate, or if any element
/// in the tree is malformed.
#[must_use]
pub fn extract_document(document_value: &Value, heap: &Heap) -> Option<Document> {
    let document_id = object_id_of(document_value)?;
    let document_obj = heap.object(document_id)?;
    let root_value = document_obj.get("documentElement")?;
    let (root_id, arena) = extract_element(root_value, Arena::new(), heap)?;
    let (doc_id, arena) = arena.alloc(Node::Document(DocumentData::new(vec![root_id])));
    let arena = reparent(arena, root_id, doc_id);
    Some(Document::new(arena, doc_id))
}

fn extract_element(value: &Value, arena: Arena, heap: &Heap) -> Option<(NodeId, Arena)> {
    let element_id = object_id_of(value)?;
    let object = heap.object(element_id)?;
    let tag_name = string_property(object, "tagName");
    let attributes = collect_attributes(object, heap);
    let text_content = string_property(object, "textContent");
    let child_values = collect_child_values(object, heap);
    let (child_ids, arena) = extract_children(child_values, arena, heap)?;
    let (final_children, arena) = if child_ids.is_empty() && !text_content.is_empty() {
        let (text_id, arena) = arena.alloc(Node::Text(TextData::new(text_content, None)));
        (vec![text_id], arena)
    } else {
        (child_ids, arena)
    };
    let element_data = ElementData::new(tag_name, attributes, final_children.clone(), None);
    let (this_id, arena) = arena.alloc(Node::Element(element_data));
    let arena = final_children
        .into_iter()
        .fold(arena, |arena, child_id| reparent(arena, child_id, this_id));
    Some((this_id, arena))
}

fn extract_children(
    child_values: Vec<Value>,
    arena: Arena,
    heap: &Heap,
) -> Option<(Vec<NodeId>, Arena)> {
    child_values.into_iter().try_fold(
        (Vec::<NodeId>::new(), arena),
        |(acc, arena), child_value| {
            let (child_id, new_arena) = extract_element(&child_value, arena, heap)?;
            let extended: Vec<NodeId> = acc.into_iter().chain(std::iter::once(child_id)).collect();
            Some((extended, new_arena))
        },
    )
}

fn reparent(arena: Arena, child_id: NodeId, new_parent: NodeId) -> Arena {
    arena
        .get(child_id)
        .cloned()
        .and_then(|node| make_reparented_node(node, new_parent))
        .into_iter()
        .fold(arena, |arena, new_node| {
            arena
                .store(child_id, new_node)
                .unwrap_or_else(|original| original)
        })
}

fn make_reparented_node(node: Node, new_parent: NodeId) -> Option<Node> {
    match node {
        Node::Element(element) => Some(Node::Element(ElementData::new(
            element.name().to_owned(),
            element.attributes().to_vec(),
            element.children().to_vec(),
            Some(new_parent),
        ))),
        Node::Text(text) => Some(Node::Text(TextData::new(
            text.content().to_owned(),
            Some(new_parent),
        ))),
        Node::Comment(comment) => Some(Node::Comment(CommentData::new(
            comment.text().to_owned(),
            Some(new_parent),
        ))),
        Node::Document(_data) => None,
    }
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

fn string_property(object: &Object, key: &str) -> String {
    match object.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(_) | None => String::new(),
    }
}

fn collect_attributes(object: &Object, heap: &Heap) -> Vec<(String, String)> {
    object
        .get("__attributes")
        .and_then(object_id_of)
        .and_then(|attrs_id| heap.object(attrs_id))
        .map(|attrs| {
            attrs
                .properties()
                .iter()
                .filter_map(|(name, value)| match value {
                    Value::String(string_value) => Some((name.clone(), string_value.clone())),
                    Value::Undefined
                    | Value::Null
                    | Value::Boolean(_)
                    | Value::Number(_)
                    | Value::Object(_)
                    | Value::Function(_)
                    | Value::Native(_)
                    | Value::Promise(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn collect_child_values(object: &Object, heap: &Heap) -> Vec<Value> {
    object
        .get("children")
        .and_then(object_id_of)
        .and_then(|children_id| heap.object(children_id))
        .map(|children| {
            let length = array_length(children);
            (0..length)
                .filter_map(|index| children.get(&format!("{index}")).cloned())
                .collect()
        })
        .unwrap_or_default()
}

fn array_length(array: &Object) -> u32 {
    match array.get("length") {
        Some(Value::Number(n)) if n.is_finite() && *n >= 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let length = *n as u32;
            length
        }
        Some(_) | None => 0,
    }
}

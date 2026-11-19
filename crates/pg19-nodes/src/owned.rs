use crate::*;
use std::ops::{Deref, DerefMut};

/// A PostgreSQL record with generated ownership information.
pub trait NodeRecord: Edges {}

pub trait Edges {
    fn detach(&mut self, pending: &mut Vec<Node>);
}

macro_rules! record_edges {
    ($($record:ident { $($field:ident),* } { $($plain:ident),* })*) => {$(
        impl NodeRecord for $record {}
        impl Edges for $record {
            fn detach(&mut self, #[allow(unused_variables)] pending: &mut Vec<Node>) {
                $(self.$field.detach(pending);)*
            }
        }
    )*};
}

include!("edges.in");

impl From<Box<Node>> for Node {
    fn from(node: Box<Node>) -> Self {
        *node
    }
}

impl<T> Edges for Option<Box<T>>
where
    Node: From<Box<T>>,
{
    fn detach(&mut self, pending: &mut Vec<Node>) {
        if let Some(child) = self.take() {
            pending.push(child.into());
        }
    }
}

impl<T: NodeRecord> Edges for Option<NodeBox<T>>
where
    Node: From<NodeBox<T>>,
{
    fn detach(&mut self, pending: &mut Vec<Node>) {
        if let Some(child) = self.take() {
            pending.push(child.into());
        }
    }
}

impl Edges for Option<Arc<Node>> {
    fn detach(&mut self, pending: &mut Vec<Node>) {
        if let Some(node) = self.take().and_then(Arc::into_inner) {
            pending.push(node);
        }
    }
}

impl Edges for Vec<Node> {
    fn detach(&mut self, pending: &mut Vec<Node>) {
        pending.append(self);
    }
}

fn drain(pending: &mut Vec<Node>) {
    while let Some(node) = pending.pop() {
        node.detach(pending);
    }
}

/// Owns a record and frees its descendants iteratively. Use `into_inner` to
/// move fields out of a matched record, or `into_box` to reuse the allocation.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeBox<T: NodeRecord>(Option<Box<T>>);

impl<T: NodeRecord> NodeBox<T> {
    pub fn into_box(mut self) -> Box<T> {
        self.0.take().expect("a live NodeBox owns its record")
    }

    pub fn into_inner(self) -> T {
        *self.into_box()
    }
}

impl<T: NodeRecord> Drop for NodeBox<T> {
    fn drop(&mut self) {
        if let Some(record) = self.0.as_deref_mut() {
            let mut pending = Vec::new();
            record.detach(&mut pending);
            drain(&mut pending);
        }
    }
}

impl<T: NodeRecord> From<T> for NodeBox<T> {
    fn from(value: T) -> Self {
        Self(Some(Box::new(value)))
    }
}

impl<T: NodeRecord> From<Box<T>> for NodeBox<T> {
    fn from(value: Box<T>) -> Self {
        Self(Some(value))
    }
}

impl<T: NodeRecord> Deref for NodeBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0.as_deref().expect("a live NodeBox owns its record")
    }
}

impl<T: NodeRecord> DerefMut for NodeBox<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.0
            .as_deref_mut()
            .expect("a live NodeBox owns its record")
    }
}

impl<T: NodeRecord> AsRef<T> for NodeBox<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: NodeRecord> AsMut<T> for NodeBox<T> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

impl<T: NodeRecord + Default> Default for NodeBox<T> {
    fn default() -> Self {
        Self::from(T::default())
    }
}

/// An owned `Node::List` payload. Ordinary record list fields use `Vec<Node>`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeList(Vec<Node>);

impl NodeList {
    pub fn into_vec(mut self) -> Vec<Node> {
        std::mem::take(&mut self.0)
    }
}

impl Drop for NodeList {
    fn drop(&mut self) {
        drain(&mut self.0);
    }
}

impl From<Vec<Node>> for Node {
    fn from(nodes: Vec<Node>) -> Self {
        Self::List(nodes.into())
    }
}

impl From<Vec<Node>> for NodeList {
    fn from(nodes: Vec<Node>) -> Self {
        Self(nodes)
    }
}

impl From<NodeList> for Vec<Node> {
    fn from(nodes: NodeList) -> Self {
        nodes.into_vec()
    }
}

impl Deref for NodeList {
    type Target = Vec<Node>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NodeList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AsRef<[Node]> for NodeList {
    fn as_ref(&self) -> &[Node] {
        self.as_slice()
    }
}

impl FromIterator<Node> for NodeList {
    fn from_iter<T: IntoIterator<Item = Node>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Extend<Node> for NodeList {
    fn extend<T: IntoIterator<Item = Node>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

impl IntoIterator for NodeList {
    type Item = Node;
    type IntoIter = std::vec::IntoIter<Node>;
    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

impl<'a> IntoIterator for &'a NodeList {
    type Item = &'a Node;
    type IntoIter = std::slice::Iter<'a, Node>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut NodeList {
    type Item = &'a mut Node;
    type IntoIter = std::slice::IterMut<'a, Node>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

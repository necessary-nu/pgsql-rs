use super::storage::Local;
use allocator_api2::vec::Vec as Buffer;
use std::{
    fmt,
    mem::{needs_drop, ManuallyDrop},
    ops::{Deref, DerefMut},
};

pub(crate) struct Vec<T>(ManuallyDrop<Buffer<T, Local>>);
impl<T> Vec<T> {
    pub(crate) fn new() -> Self {
        const { assert!(!needs_drop::<T>()) };
        Self(ManuallyDrop::new(Buffer::new_in(Local)))
    }
    pub(crate) fn with_capacity(cap: usize) -> Self {
        let mut v = Self::new();
        v.0.reserve_exact(cap);
        v
    }
    pub(crate) fn as_slice(&self) -> &[T] {
        self
    }
    pub(crate) fn into_vec(self) -> Self {
        self
    }
    pub(crate) fn push(&mut self, v: T) {
        self.0.push(v)
    }
    pub(crate) fn insert(&mut self, i: usize, v: T) {
        self.0.insert(i, v)
    }
    pub(crate) fn split_off(&mut self, i: usize) -> Self {
        Self(ManuallyDrop::new(self.0.split_off(i)))
    }
    pub(crate) fn append(&mut self, other: &mut Self) {
        self.0.append(&mut other.0)
    }
}
impl<T> Default for Vec<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> Deref for Vec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.0
    }
}
impl<T> DerefMut for Vec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.0
    }
}
impl<T> Extend<T> for Vec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, it: I) {
        self.0.extend(it)
    }
}
impl<T> FromIterator<T> for Vec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(it: I) -> Self {
        let mut v = Self::new();
        v.extend(it);
        v
    }
}
impl<T, const N: usize> From<[T; N]> for Vec<T> {
    fn from(a: [T; N]) -> Self {
        a.into_iter().collect()
    }
}
impl<T: Clone> Clone for Vec<T> {
    fn clone(&self) -> Self {
        self.iter().cloned().collect()
    }
}
impl<T: fmt::Debug> fmt::Debug for Vec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}
impl<T: PartialEq> PartialEq for Vec<T> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}
impl<'a, T> IntoIterator for &'a Vec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T> IntoIterator for &'a mut Vec<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
pub(crate) struct IntoIter<T>(allocator_api2::vec::IntoIter<T, Local>);
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.0.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}
impl<T> ExactSizeIterator for IntoIter<T> {}
impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<T> {
        self.0.next_back()
    }
}
impl<T> IntoIterator for Vec<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIter(ManuallyDrop::into_inner(self.0).into_iter())
    }
}

impl<T, const N: usize> TryFrom<Vec<T>> for [T; N] {
    type Error = Vec<T>;
    fn try_from(v: Vec<T>) -> Result<Self, Self::Error> {
        ManuallyDrop::into_inner(v.0)
            .try_into()
            .map_err(|buf| Vec(ManuallyDrop::new(buf)))
    }
}

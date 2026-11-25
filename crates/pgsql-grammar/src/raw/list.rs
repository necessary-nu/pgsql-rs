use super::storage::{read, write, Arena, Stored};
use std::marker::PhantomData;

/// A range in the element type's pool. A growing list may move to a new range
/// when another list has taken the end of the pool.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Vec<T> {
    start: usize,
    len: usize,
    capacity: usize,
    marker: PhantomData<fn() -> T>,
}
impl<T> Default for Vec<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T> Vec<T> {
    pub(crate) fn new() -> Self {
        Self {
            start: 0,
            len: 0,
            capacity: 0,
            marker: PhantomData,
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }
}
impl<T: Stored + Copy + Default> Vec<T> {
    pub(crate) fn slice(self, arena: &Arena) -> &[T] {
        &T::pool(arena)[self.start..self.start + self.len]
    }
    pub(crate) fn get(self, i: usize) -> T {
        read(|a| self.slice(a)[i])
    }
    pub(crate) fn set(self, i: usize, value: T) {
        assert!(i < self.len);
        write(|a| T::pool_mut(a)[self.start + i] = value)
    }
    pub(crate) fn with_capacity(cap: usize) -> Self {
        let mut list = Self::new();
        if cap != 0 {
            write(|a| list.reserve(a, cap));
        }
        list
    }
    fn reserve(&mut self, a: &mut Arena, need: usize) {
        if need <= self.capacity {
            return;
        }
        let cap = need.max(self.capacity.saturating_mul(2));
        if self.capacity != 0 && self.start + self.capacity == T::pool(a).len() {
            a.append(std::iter::repeat_n(T::default(), cap - self.capacity));
        } else {
            let start = a.append(std::iter::repeat_n(T::default(), cap));
            T::pool_mut(a).copy_within(self.start..self.start + self.len, start);
            self.start = start;
        }
        self.capacity = cap;
    }
    pub(crate) fn push(&mut self, value: T) {
        write(|a| {
            if self.capacity == 0 {
                self.start = a.push(value);
                self.capacity = 1;
            } else if self.len == self.capacity && self.start + self.len == T::pool(a).len() {
                a.push(value);
                self.capacity += 1;
            } else {
                self.reserve(a, self.len + 1);
                T::pool_mut(a)[self.start + self.len] = value;
            }
            self.len += 1;
        })
    }
    pub(crate) fn insert(&mut self, i: usize, value: T) {
        assert!(i <= self.len);
        write(|a| {
            self.reserve(a, self.len + 1);
            let pool = T::pool_mut(a);
            pool.copy_within(self.start + i..self.start + self.len, self.start + i + 1);
            pool[self.start + i] = value;
            self.len += 1;
        });
    }
    pub(crate) fn split_off(&mut self, i: usize) -> Self {
        assert!(i <= self.len);
        let tail = Self {
            start: self.start + i,
            len: self.len - i,
            capacity: self.capacity - i,
            marker: PhantomData,
        };
        self.len = i;
        self.capacity = i;
        tail
    }
    pub(crate) fn append(&mut self, other: Self) {
        if self.is_empty() {
            *self = other;
            return;
        }
        if other.is_empty() {
            return;
        }
        write(|a| {
            self.reserve(a, self.len + other.len);
            T::pool_mut(a).copy_within(other.start..other.start + other.len, self.start + self.len);
            self.len += other.len;
        });
    }
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = T> + ExactSizeIterator + use<T> {
        (*self).into_iter()
    }
    pub(crate) fn first(self) -> Option<T> {
        self.iter().next()
    }
    pub(crate) fn last(self) -> Option<T> {
        self.iter().next_back()
    }
}
impl<T: Stored + Copy + Default> Extend<T> for Vec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, it: I) {
        for v in it {
            self.push(v)
        }
    }
}
impl<T: Stored + Copy + Default> FromIterator<T> for Vec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(it: I) -> Self {
        let it = it.into_iter();
        let mut result = Self::with_capacity(it.size_hint().0);
        result.extend(it);
        result
    }
}
impl<T: Stored + Copy + Default, const N: usize> From<[T; N]> for Vec<T> {
    fn from(arr: [T; N]) -> Self {
        if N == 0 {
            return Self::new();
        }
        let start = write(|a| a.append(arr));
        Self {
            start,
            len: N,
            capacity: N,
            marker: PhantomData,
        }
    }
}
impl Vec<super::String> {
    pub(crate) fn join(self, separator: &str) -> std::string::String {
        read(|a| {
            let mut result = std::string::String::new();
            for (i, word) in self.slice(a).iter().enumerate() {
                if i > 0 {
                    result.push_str(separator)
                }
                result.push_str(word.get(a))
            }
            result
        })
    }
}
pub(crate) struct IntoIter<T> {
    list: Vec<T>,
    range: std::ops::Range<usize>,
}
impl<T: Stored + Copy + Default> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.range.next().map(|i| self.list.get(i))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}
impl<T: Stored + Copy + Default> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<T> {
        self.range.next_back().map(|i| self.list.get(i))
    }
}
impl<T: Stored + Copy + Default> ExactSizeIterator for IntoIter<T> {}
impl<T: Stored + Copy + Default> IntoIterator for Vec<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            list: self,
            range: 0..self.len,
        }
    }
}
impl<T: Stored + Copy + Default> IntoIterator for &Vec<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        (*self).into_iter()
    }
}
impl<T: Stored + Copy + Default, const N: usize> TryFrom<Vec<T>> for [T; N] {
    type Error = Vec<T>;
    fn try_from(list: Vec<T>) -> Result<Self, Self::Error> {
        read(|a| list.slice(a).try_into().map_err(|_| list))
    }
}

impl<T: Stored + Copy + Default + PartialEq> PartialEq for Vec<T> {
    fn eq(&self, other: &Self) -> bool {
        read(|a| self.slice(a) == other.slice(a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::Region;

    #[test]
    fn growing_interleaved_lists_preserves_split_ranges() {
        let (region, (left, right)) = Region::build(|| {
            let mut left = Vec::from([1_i32, 2, 3]);
            let mut right = left.split_off(1);
            for n in 4..32 {
                left.push(-n);
                right.push(n);
            }
            let tail = right.split_off(2);
            left.append(tail);
            (left, right)
        });
        assert_eq!(right.slice(region.arena()), &[2, 3]);
        let expected: std::vec::Vec<_> = [1]
            .into_iter()
            .chain((4..32).map(|n| -n))
            .chain(4..32)
            .collect();
        assert_eq!(left.slice(region.arena()), expected);
    }
}

//! A parsed tree owns the pools addressed by its private, typed indices.
use super::*;
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    num::NonZeroUsize,
    vec::Vec as Buffer,
};

pub(crate) trait Stored: Sized {
    fn pool(arena: &Arena) -> &Buffer<Self>;
    fn pool_mut(arena: &mut Arena) -> &mut Buffer<Self>;
}

macro_rules! pools {
    ($($name:ident),*) => {
        #[derive(Default)]
        #[allow(non_snake_case)]
        pub(crate) struct Arena {
            $($name: Buffer<$name>,)*
            text: std::string::String,
            active: Buffer<fn(&mut Arena)>,
            reserved: usize,
        }
        $(impl Stored for $name {
            #[inline] fn pool(arena: &Arena) -> &Buffer<Self> { &arena.$name }
            #[inline] fn pool_mut(arena: &mut Arena) -> &mut Buffer<Self> { &mut arena.$name }
        })*
    };
}
macro_rules! node_types {
    (values { $($values:tt)* } records { $($record:ident ($($tag:tt)*) { $($fields:tt)* })* }) => {
        pools!(String, Node, SelectLimit, u8, i32, u32, $($record),*);
    };
}
use crate::limits::SelectLimit;
include!("../../../pgsql-nodes/src/nodes.in");

impl Arena {
    #[inline]
    pub(super) fn push<T: Stored>(&mut self, value: T) -> usize {
        let start = T::pool(self).len();
        if start == 0 {
            self.active.push(|arena| T::pool_mut(arena).clear());
        }
        let pool = T::pool_mut(self);
        let old_capacity = pool.capacity();
        pool.push(value);
        if old_capacity == start {
            self.reserved += (pool.capacity() - old_capacity) * std::mem::size_of::<T>();
        }
        start
    }

    pub(super) fn append<T: Stored>(&mut self, values: impl IntoIterator<Item = T>) -> usize {
        let start = T::pool(self).len();
        if start == 0 {
            self.active.push(|a| T::pool_mut(a).clear());
        }
        let pool = T::pool_mut(self);
        let old = pool.capacity();
        pool.extend(values);
        self.reserved += (pool.capacity() - old) * std::mem::size_of::<T>();
        start
    }
}

thread_local! {
    // Reductions allocate into CURRENT. A finished tree takes the arena with it.
    static CURRENT: RefCell<Option<std::boxed::Box<Arena>>> = const { RefCell::new(None) };
    static SPARE: Cell<Option<std::boxed::Box<Arena>>> = const { Cell::new(None) };
}

pub(crate) struct Region(Option<std::boxed::Box<Arena>>);
impl Region {
    pub(crate) fn build<R>(f: impl FnOnce() -> R) -> (Self, R) {
        struct Scope(Option<std::boxed::Box<Arena>>);
        impl Drop for Scope {
            fn drop(&mut self) {
                let active = CURRENT.replace(self.0.take());
                drop(Region(active));
            }
        }
        let arena = SPARE.with(Cell::take).unwrap_or_default();
        let scope = Scope(CURRENT.replace(Some(arena)));
        let result = f();
        let region = Self(CURRENT.take());
        drop(scope);
        (region, result)
    }
    pub(crate) fn arena(&self) -> &Arena {
        self.0.as_deref().unwrap()
    }
}
impl Drop for Region {
    fn drop(&mut self) {
        if let Some(mut arena) = self.0.take() {
            if arena.reserved <= 128 * 1024 {
                while let Some(clear) = arena.active.pop() {
                    clear(&mut arena);
                }
                arena.text.clear();
                let _ = SPARE.try_with(|spare| spare.set(Some(arena)));
            }
        }
    }
}
#[inline]
pub(crate) fn read<R>(f: impl FnOnce(&Arena) -> R) -> R {
    CURRENT.with_borrow(|slot| f(slot.as_deref().expect("active grammar reduction")))
}
#[inline]
pub(super) fn write<R>(f: impl FnOnce(&mut Arena) -> R) -> R {
    CURRENT.with_borrow_mut(|slot| f(slot.as_deref_mut().expect("active grammar reduction")))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Id<T>(NonZeroUsize, PhantomData<fn() -> T>);
impl<T> Id<T> {
    #[inline]
    fn index(&self) -> usize {
        self.0.get() - 1
    }
}
impl<T: Stored> Id<T> {
    #[inline]
    pub(crate) fn new(value: T) -> Self {
        let i = write(|arena| arena.push(value));
        Self(NonZeroUsize::new(i + 1).unwrap(), PhantomData)
    }
    #[inline]
    pub(crate) fn get(self, arena: &Arena) -> &T {
        &T::pool(arena)[self.index()]
    }
}
impl<T: Stored + Copy> Id<T> {
    #[inline]
    pub(crate) fn load(self) -> T {
        read(|a| *self.get(a))
    }
    pub(crate) fn set(self, value: T) {
        write(|arena| T::pool_mut(arena)[self.index()] = value);
    }
    pub(crate) fn set_field<F>(self, field: impl FnOnce(&mut T) -> &mut F, value: F) {
        write(|arena| *field(&mut T::pool_mut(arena)[self.index()]) = value);
    }
}
impl<T: Stored> From<T> for Id<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum String {
    Static(&'static str),
    Text { start: usize, len: usize },
}
impl Default for String {
    fn default() -> Self {
        Self::Static("")
    }
}
impl String {
    pub(crate) fn get(self, arena: &Arena) -> &str {
        match self {
            Self::Static(s) => s,
            Self::Text { start, len } => &arena.text[start..start + len],
        }
    }
    pub(crate) fn owned(self) -> std::string::String {
        read(|a| self.get(a).to_owned())
    }
    fn store(s: &str, fold: bool) -> Self {
        write(|arena| {
            let start = arena.text.len();
            let old = arena.text.capacity();
            arena.text.push_str(s);
            arena.reserved += arena.text.capacity() - old;
            if fold {
                arena.text[start..].make_ascii_lowercase();
            }
            Self::Text {
                start,
                len: s.len(),
            }
        })
    }
}
impl pgsql_lexer::ScannerText for String {
    fn with_str<R>(&self, f: impl FnOnce(&str) -> R) -> R {
        read(|arena| f(self.get(arena)))
    }
    fn from_identifier(word: &str) -> Self {
        Self::store(word, true)
    }
}
impl From<&str> for String {
    fn from(s: &str) -> Self {
        if s.is_empty() {
            return Self::default();
        }
        Self::store(s, false)
    }
}
impl From<std::string::String> for String {
    fn from(s: std::string::String) -> Self {
        s.as_str().into()
    }
}
impl PartialEq for String {
    fn eq(&self, other: &Self) -> bool {
        read(|a| self.get(a) == other.get(a))
    }
}
impl PartialEq<str> for String {
    fn eq(&self, s: &str) -> bool {
        read(|a| self.get(a) == s)
    }
}
impl PartialEq<&str> for String {
    fn eq(&self, s: &&str) -> bool {
        self == *s
    }
}
impl std::fmt::Display for String {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        read(|a| f.write_str(self.get(a)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shared<T>(Id<T>);
impl<T> From<Id<T>> for Shared<T> {
    fn from(id: Id<T>) -> Self {
        Self(id)
    }
}
impl<T: Stored + Copy> Shared<T> {
    pub(crate) fn get(self, arena: &Arena) -> &T {
        self.0.get(arena)
    }
    pub(crate) fn index(self) -> usize {
        self.0.index()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_parse_restores_outer_arena_after_unwinding() {
        let (outer, id) = Region::build(|| {
            let id = Id::new(17_i32);
            let inner = std::panic::catch_unwind(|| {
                Region::build(|| {
                    let inner = Id::new(31_i32);
                    assert_eq!(inner.load(), 31);
                    panic!("abort inner parse");
                })
            });
            assert!(inner.is_err());
            assert_eq!(id.load(), 17);
            id
        });
        let (other, other_id) = Region::build(|| Id::new(42_i32));
        assert_eq!(other_id.get(other.arena()), &42);
        assert_eq!(id.get(outer.arena()), &17);
    }
}

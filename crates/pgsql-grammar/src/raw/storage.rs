//! Storage for the private reduction model. The public tree owns the region.
//! Its read methods borrow the tree. Only ordinary owned nodes can escape it.
use allocator_api2::alloc::{AllocError, Allocator};
use bumpalo::Bump;
use std::{
    alloc::Layout,
    cell::Cell,
    fmt,
    mem::needs_drop,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

thread_local! {
    static CURRENT: Cell<Option<NonNull<Bump>>> = const { Cell::new(None) };
    static SPARE: Cell<Option<Bump>> = const { Cell::new(None) };
}

pub(crate) struct Region(Option<Bump>);

impl Region {
    pub(crate) fn new() -> Self {
        Self(Some(
            SPARE
                .with(Cell::take)
                .unwrap_or_else(|| Bump::with_capacity(8192)),
        ))
    }

    pub(crate) fn build<R>(&self, build: impl FnOnce() -> R) -> R {
        struct Scope<'a>(&'a Cell<Option<NonNull<Bump>>>, Option<NonNull<Bump>>);
        impl Drop for Scope<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        CURRENT.with(|slot| {
            let previous = slot.replace(Some(NonNull::from(self.0.as_ref().unwrap())));
            let _scope = Scope(slot, previous);
            build()
        })
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        let mut bump = self.0.take().unwrap();
        if bump.allocated_bytes() <= 128 * 1024 {
            bump.reset();
            let _ = SPARE.try_with(|slot| slot.set(Some(bump)));
        }
    }
}

#[inline]
fn in_region<R>(f: impl FnOnce(&Bump) -> R) -> R {
    CURRENT.with(|slot| {
        let ptr = slot.get().expect("arena allocation outside a reduction");
        // SAFETY: build borrows the region and restores the previous pointer before returning.
        f(unsafe { ptr.as_ref() })
    })
}

// The allocator is private. Buffers can grow only during their originating parse,
// and are discarded together when the result (or a failed parse) releases its region.
#[derive(Clone, Copy)]
pub(super) struct Local;
unsafe impl Allocator for Local {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        Ok(NonNull::slice_from_raw_parts(
            in_region(|bump| bump.alloc_layout(layout)),
            layout.size(),
        ))
    }
    unsafe fn deallocate(&self, _: NonNull<u8>, _: Layout) {}
}

#[derive(Debug)]
pub(crate) struct Box<T>(NonNull<T>);
impl<T> Box<T> {
    #[inline]
    pub(crate) fn new(value: T) -> Self {
        const {
            assert!(!needs_drop::<T>());
        }
        Self(in_region(|bump| NonNull::from(bump.alloc(value))))
    }
    pub(crate) fn into_inner(self) -> T {
        // SAFETY: this consumes the only owning handle. Bump has no element destructors.
        unsafe { self.0.read() }
    }
    pub(crate) fn into_box(self) -> Self {
        self
    }
}
impl<T> From<T> for Box<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}
impl<T> Deref for Box<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: handles are private to the parser and borrowed views keep the region alive.
        unsafe { self.0.as_ref() }
    }
}
impl<T> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: this handle uniquely owns the pointee; cloning allocates a fresh value.
        unsafe { self.0.as_mut() }
    }
}
impl<T> AsRef<T> for Box<T> {
    fn as_ref(&self) -> &T {
        self
    }
}
impl<T: Clone> Clone for Box<T> {
    fn clone(&self) -> Self {
        Self::new((**self).clone())
    }
}
impl<T: PartialEq> PartialEq for Box<T> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

#[derive(Debug)]
pub(crate) struct Arc<T>(NonNull<T>);
impl<T> From<Box<T>> for Arc<T> {
    fn from(owner: Box<T>) -> Self {
        Self(owner.0)
    }
}
impl<T> Arc<T> {
    pub(crate) fn as_ptr(this: &Self) -> *const T {
        this.0.as_ptr()
    }
    pub(crate) fn ptr_eq(a: &Self, b: &Self) -> bool {
        a.0 == b.0
    }
}
impl<T> Clone for Arc<T> {
    fn clone(&self) -> Self {
        Self(self.0)
    }
}
impl<T> Deref for Arc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: shared handles offer only immutable access within the region's lifetime.
        unsafe { self.0.as_ref() }
    }
}
impl<T: PartialEq> PartialEq for Arc<T> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

#[derive(Clone, Copy)]
pub(crate) struct String {
    text: NonNull<str>,
}
impl String {
    pub(crate) fn as_str(&self) -> &str {
        self
    }
    pub(crate) fn from_static(text: &'static str) -> Self {
        Self {
            text: NonNull::from(text),
        }
    }
}
impl Default for String {
    fn default() -> Self {
        Self::from_static("")
    }
}
impl From<&str> for String {
    fn from(text: &str) -> Self {
        if text.is_empty() {
            return Self::default();
        }
        Self {
            text: in_region(|bump| NonNull::from(bump.alloc_str(text))),
        }
    }
}
impl pgsql_lexer::TokenText for String {
    fn lowercase_identifier(word: &str) -> Self {
        in_region(|bump| {
            let text = bump.alloc_str(word);
            text.make_ascii_lowercase();
            Self {
                text: NonNull::from(text),
            }
        })
    }
}
impl From<std::string::String> for String {
    fn from(s: std::string::String) -> Self {
        s.as_str().into()
    }
}
impl Deref for String {
    type Target = str;
    fn deref(&self) -> &str {
        // SAFETY: construction preserves UTF-8 and the public borrow keeps the region alive.
        unsafe { self.text.as_ref() }
    }
}
impl fmt::Debug for String {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}
impl fmt::Display for String {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
impl PartialEq for String {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}
impl PartialEq<str> for String {
    fn eq(&self, other: &str) -> bool {
        &**self == other
    }
}
impl PartialEq<&str> for String {
    fn eq(&self, other: &&str) -> bool {
        &**self == *other
    }
}
impl AsRef<str> for String {
    fn as_ref(&self) -> &str {
        self
    }
}
impl std::borrow::Borrow<str> for String {
    fn borrow(&self) -> &str {
        self
    }
}

// SAFETY: a completed tree exclusively owns its arena's chunks. Only immutable
// views escape. The bump cursor is touched again only by the tree's destructor.
unsafe impl Send for crate::RawTree {}
unsafe impl Sync for crate::RawTree {}
// SAFETY: the view borrows that frozen tree for its full lifetime.
unsafe impl Send for crate::RawStatement<'_> {}
unsafe impl Sync for crate::RawStatement<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unwinding_a_nested_build_restores_the_outer_region() {
        let outer = Region::new();
        let text = outer.build(|| {
            let failed = std::panic::catch_unwind(|| {
                let inner = Region::new();
                inner.build(|| {
                    let _partial = Box::new(String::from("inner"));
                    panic!("abort the inner build");
                });
            });
            assert!(failed.is_err());
            String::from("outer")
        });
        let other = Region::new();
        other.build(|| assert_eq!(String::from("other").as_str(), "other"));
        assert_eq!(text.as_str(), "outer");
    }
}

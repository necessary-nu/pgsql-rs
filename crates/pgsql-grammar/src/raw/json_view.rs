use super::*;
pub(super) type Context = Arena;
pub(super) type ListView = Vec<Node>;
pub(super) type TextView = String;
pub(super) trait Pointer {
    type Target;
    fn resolve<'a>(&'a self, arena: &'a Arena) -> &'a Self::Target;
}
impl<T: Stored + Copy> Pointer for Box<T> {
    type Target = T;
    fn resolve<'a>(&'a self, a: &'a Arena) -> &'a T {
        self.get(a)
    }
}
impl<T: Stored + Copy> Pointer for Arc<T> {
    type Target = T;
    fn resolve<'a>(&'a self, a: &'a Arena) -> &'a T {
        self.get(a)
    }
}
impl Arena {
    pub(super) fn string<'a>(&'a self, s: &String) -> &'a str {
        s.get(self)
    }
    pub(super) fn items<'a, T: Stored + Copy + Default>(&'a self, v: &Vec<T>) -> &'a [T] {
        v.slice(self)
    }
    pub(super) fn bitmap<'a>(&'a self, v: &Bitmapset) -> &'a [u32] {
        v.slice(self)
    }
}

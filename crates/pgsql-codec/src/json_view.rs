use super::Bitmapset;
pub(super) type ListView = [super::Node];
pub(super) type TextView = str;
pub(super) struct Context;
pub(super) trait Pointer {
    type Target: Sized;
    fn resolve<'a>(&'a self, context: &'a Context) -> &'a Self::Target;
}
impl<P: std::ops::Deref> Pointer for P
where
    P::Target: Sized,
{
    type Target = P::Target;
    fn resolve<'a>(&'a self, _: &'a Context) -> &'a Self::Target {
        self
    }
}
impl Context {
    pub(super) fn string<'a>(&self, s: &'a str) -> &'a str {
        s
    }
    pub(super) fn items<'a, T>(&self, v: &'a [T]) -> &'a [T] {
        v
    }
    pub(super) fn bitmap<'a>(&self, v: &'a Bitmapset) -> &'a Bitmapset {
        v
    }
}

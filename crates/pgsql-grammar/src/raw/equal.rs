use super::json_view::Pointer;
include!("../../../pgsql-nodes/src/equal.rs");
type Context = Arena;
impl<T: Tagged + Stored + Copy> Field for Option<Box<T>> {
    fn check<'a>(&'a self, other: &'a Self, w: &mut Work<'a>) -> bool {
        nullable(
            self.map(|n| n.get(w.context)),
            other.map(|n| n.get(w.context)),
            w,
        )
    }
}
impl Field for Option<Arc<Node>> {
    fn check<'a>(&'a self, other: &'a Self, w: &mut Work<'a>) -> bool {
        if let (Some(a), Some(b)) = (self, other) {
            if a.index() == b.index() || !w.shared.insert((a.index(), b.index())) {
                return true;
            }
        }
        nullable(
            self.map(|n| n.get(w.context)),
            other.map(|n| n.get(w.context)),
            w,
        )
    }
}
impl Field for Vec<Node> {
    fn check<'a>(&'a self, other: &'a Self, w: &mut Work<'a>) -> bool {
        self.slice(w.context).check(other.slice(w.context), w)
    }
}
impl TypeName {
    pub(crate) fn pg_equal(&self, other: &Self, arena: &Arena) -> Result<bool, NodeTag> {
        compare(self, other, arena)
    }
}

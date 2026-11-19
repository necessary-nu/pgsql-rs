/// PG's signed byte offset into the UTF-8 SQL source. Zero is a valid offset.
// [spec:pgq:req:tree.node-locations]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Location(i32);

impl Location {
    pub const UNKNOWN: Self = Self(-1);

    pub fn from_byte_offset(byte: usize) -> Option<Self> {
        i32::try_from(byte).ok().map(Self)
    }

    pub fn from_pg(offset: i32) -> Option<Self> {
        (offset >= -1).then_some(Self(offset))
    }

    pub fn as_pg(self) -> i32 {
        self.0
    }

    pub fn byte_offset(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }
}

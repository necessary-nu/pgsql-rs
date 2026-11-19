use crate::catalog::COLUMNS;

const CLASS: &[u8; 256] = include_bytes!("../data/classes.bin");
const ACTION: &[u8] = include_bytes!("../data/actions.bin");
const EDGES: &[u8] = include_bytes!("../data/edges.bin");

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum State {
    Initial = 1,
    Bit,
    Comment,
    Identifier,
    Hex,
    Standard,
    AfterQuote,
    Escape,
    UnicodeIdentifier = 10,
    UnicodeString,
    Surrogate,
}

/// Flex's PostgreSQL scanner has no backup states. Its accepting action is
/// given by the state just before the first missing transition.
pub(crate) fn longest(state: State, sql: &[u8], start: usize) -> (u8, usize) {
    let mut state = state as usize;
    let mut end = start;
    for &byte in &sql[start..] {
        let index = 2 * ((state - 1) * COLUMNS + usize::from(CLASS[usize::from(byte)]));
        let next = u16::from_le_bytes([EDGES[index], EDGES[index + 1]]);
        if next == 0 {
            break;
        }
        state = usize::from(next);
        end += 1;
    }
    (ACTION[state], end)
}

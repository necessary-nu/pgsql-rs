use crate::catalog::COLUMNS;

const ACTION: &[u8] = include_bytes!("../data/actions.bin");
const CLASS: &[u8; 256] = include_bytes!("../data/classes.bin");
const PACKED: &[u8] = include_bytes!("../data/edges.bin");

// Expand Flex's alphabet classes at compile time. Scanning then consumes a
// byte with one dependent table load. Row zero is the jam state.
static EDGES: [[u8; 256]; ACTION.len()] = {
    assert!(ACTION.len() <= 256);
    let mut edges = [[0; 256]; ACTION.len()];
    let mut state = 1;
    while state < edges.len() {
        let mut byte = 0;
        while byte < 256 {
            let i = 2 * ((state - 1) * COLUMNS + CLASS[byte] as usize);
            let next = u16::from_le_bytes([PACKED[i], PACKED[i + 1]]);
            assert!((next as usize) < ACTION.len());
            edges[state][byte] = next as u8;
            byte += 1;
        }
        state += 1;
    }
    edges
};

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
#[inline(always)]
pub(crate) fn longest(state: State, sql: &[u8], start: usize) -> (u8, usize) {
    let mut state = state as usize;
    let mut end = start;
    for &byte in &sql[end..] {
        let next = EDGES[state][usize::from(byte)];
        if next == 0 {
            break;
        }
        state = usize::from(next);
        end += 1;
    }
    (ACTION[state], end)
}

/// Recognize the identifier loop without indexing the next DFA row for each byte.
/// Prefixes with special Flex transitions (such as U& and E') stay with the full scanner.
#[inline]
pub(crate) fn ordinary_word(bytes: &[u8]) -> Option<usize> {
    const WORD: usize =
        EDGES[EDGES[State::Initial as usize][b'a' as usize] as usize][b'a' as usize] as usize;
    let (&first, rest) = bytes.split_first()?;
    let state = usize::from(EDGES[State::Initial as usize][usize::from(first)]);
    if ACTION[state] != 71 {
        return None;
    }
    let Some((&second, rest)) = rest.split_first() else {
        return Some(1);
    };
    match usize::from(EDGES[state][usize::from(second)]) {
        0 => return Some(1),
        WORD => {}
        _ => return None,
    }
    let mut len = 2;
    for &byte in rest {
        match usize::from(EDGES[WORD][usize::from(byte)]) {
            WORD => len += 1,
            0 => return Some(len),
            _ => return None,
        }
    }
    Some(len)
}

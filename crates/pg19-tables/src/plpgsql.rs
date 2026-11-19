include!("plpgsql/layout.rs");
const TABLES: &[u8] = include_bytes!("../data/plpgsql/tables.bin");
const INVENTORY: &str = include_str!("../data/plpgsql/productions.tsv");
const SYMBOLS: &str = include_str!("../data/plpgsql/symbols.txt");
include!("lalr.rs");

pub mod token {
    include!("plpgsql/tokens.rs");
}

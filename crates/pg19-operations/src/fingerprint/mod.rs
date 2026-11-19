//! Version 3 libpg_query fingerprinting for typed PostgreSQL raw trees.
use pg19_nodes::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use xxhash_rust::xxh3::xxh3_64_with_seed;

pub const FINGERPRINT_VERSION: u64 = 3;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FingerprintOptions {
    pub ignore_aliases: bool,
    pub include_schema: bool,
    pub full_relation_name: bool,
}

impl FingerprintOptions {
    pub const PG17_COMPAT: Self = Self {
        ignore_aliases: true,
        include_schema: true,
        full_relation_name: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint(pub u64);

impl Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

#[derive(Clone, Copy, Default)]
struct Site {
    depth: usize,
    owner: Option<NodeTag>,
    edge: Option<(NodeTag, &'static str)>,
}

impl Site {
    fn child(self, field: &'static str) -> Self {
        Self {
            depth: self.depth + 1,
            edge: self.owner.map(|owner| (owner, field)),
            owner: None,
        }
    }

    fn item(self) -> Self {
        Self {
            depth: self.depth + 1,
            ..self
        }
    }
}

trait Body {
    fn emit(&self, writer: &mut Writer, at: Site);
}

struct Writer {
    stream: Vec<u8>,
    options: FingerprintOptions,
}

impl Writer {
    fn new(options: FingerprintOptions) -> Self {
        Self {
            stream: Vec::new(),
            options,
        }
    }

    fn token(&mut self, s: &str) {
        self.stream.extend_from_slice(s.as_bytes());
    }

    fn symbol(&mut self, key: &str, symbol: Option<&str>) {
        self.token(key);
        if let Some(s) = symbol {
            self.token(s);
        }
    }

    fn string(&mut self, key: &str, value: Option<&str>) {
        if let Some(s) = value {
            self.token(key);
            self.token(s);
        }
    }

    fn boolean(&mut self, key: &str, value: bool) {
        if value {
            self.token(key);
            self.token("true");
        }
    }

    fn character(&mut self, key: &str, value: u8) {
        if value != 0 {
            self.token(key);
            self.stream.push(value);
        }
    }

    fn integer<T: Display + Default + PartialEq>(&mut self, key: &str, value: T) {
        if value != T::default() {
            self.token(key);
            self.token(&value.to_string());
        }
    }

    fn float(&mut self, key: &str, value: f64) {
        if value != 0.0 {
            self.token(key);
            self.token(&format!("{value:.6}"));
        }
    }

    fn bitmap(&mut self, key: &str, values: &BTreeSet<u32>) {
        self.token(key);
        for value in values {
            self.token(&value.to_string());
        }
    }

    fn prefix_child(
        &mut self,
        key: &'static str,
        retain_empty: bool,
        emit: impl FnOnce(&mut Self),
    ) {
        let start = self.stream.len();
        self.token(key);
        let prefix_end = self.stream.len();
        emit(self);
        if self.stream.len() == prefix_end && !retain_empty {
            self.stream.truncate(start);
        }
    }

    fn pointer<T: Body>(&mut self, field: &'static str, value: Option<&T>, at: Site) {
        let at = at.child(field);
        if let Some(value) = value.filter(|_| at.depth < 100) {
            self.prefix_child(field, false, |w| value.emit(w, at));
        }
    }

    fn embedded(&mut self, field: &'static str, value: &impl Body, at: Site) {
        self.token(field);
        value.emit(
            self,
            Site {
                depth: at.depth,
                ..at.child(field)
            },
        );
    }

    fn list_field(&mut self, field: &'static str, value: &[Node], at: Site) {
        if !value.is_empty() {
            self.prefix_child(field, matches!(value, [Node::Null]), |w| {
                w.list(value, at.child(field))
            });
        }
    }

    // [spec:pgq:req:fingerprint.lists]
    fn list(&mut self, value: &[Node], at: Site) {
        let item = at.item();
        if item.depth >= 100 {
            return;
        }
        if matches!(
            at.edge,
            Some((
                _,
                "fromClause" | "targetList" | "cols" | "rexpr" | "valuesLists" | "args"
            ))
        ) {
            // Keep a child's bytes alongside its hash so nested lists are only visited once.
            let mut unique = BTreeMap::new();
            for child in value {
                let mut w = Writer::new(self.options);
                child.emit(&mut w, item);
                let digest = xxh3_64_with_seed(&w.stream, FINGERPRINT_VERSION);
                unique.entry(digest).or_insert(w.stream);
            }
            for stream in unique.into_values() {
                self.stream.extend(stream);
            }
        } else {
            for child in value {
                child.emit(self, item);
            }
        }
    }

    fn target_name(&mut self, name: Option<&str>, at: Site) {
        if at.edge != Some((NodeTag::SelectStmt, "targetList")) {
            self.string("name", name);
        }
    }

    fn expression_kind(&mut self, kind: AExprKind) {
        let symbol = match kind {
            AExprKind::AEXPR_OP_ANY | AExprKind::AEXPR_IN => AExprKind::AEXPR_OP,
            other => other,
        };
        self.symbol("kind", symbol.pg_name());
    }

    // [spec:pgq:req:fingerprint.aliases]
    fn relation(&mut self, n: &RangeVar, at: Site) {
        let dml = matches!(
            at.edge,
            Some(
                (NodeTag::SelectStmt, "fromClause")
                    | (NodeTag::InsertStmt, "relation")
                    | (NodeTag::UpdateStmt, "relation" | "fromClause")
                    | (NodeTag::DeleteStmt, "relation" | "usingClause")
                    | (NodeTag::MergeStmt, "relation" | "sourceRelation")
                    | (
                        NodeTag::JoinExpr | NodeTag::RangeTableSample | NodeTag::LockingClause,
                        _
                    )
            )
        );
        if !self.options.ignore_aliases {
            self.string(
                "aliasname",
                n.alias.as_ref().and_then(|a| a.aliasname.as_deref()),
            );
        }
        self.string("catalogname", n.catalogname.as_deref());
        self.boolean("inh", n.inh);
        let aliased = dml && !self.options.ignore_aliases && n.alias.is_some();
        if let Some(name) = n
            .relname
            .as_deref()
            .filter(|_| !aliased && n.relpersistence != b't')
        {
            self.relname(name);
        }
        self.character("relpersistence", n.relpersistence);
        if !dml || self.options.include_schema {
            self.string("schemaname", n.schemaname.as_deref());
        }
    }

    // [spec:pgq:req:fingerprint.relation-digits]
    fn relname(&mut self, name: &str) {
        self.token("relname");
        if self.options.full_relation_name {
            self.token(name);
        } else {
            let bytes = name.as_bytes();
            self.stream
                .extend(bytes.iter().enumerate().filter_map(|(i, &b)| {
                    let paired = b.is_ascii_digit()
                        && ((i > 0 && bytes[i - 1].is_ascii_digit())
                            || bytes.get(i + 1).is_some_and(u8::is_ascii_digit));
                    (!paired).then_some(b)
                }));
        }
    }
}

macro_rules! fingerprint_records {
    ($n:ident, $w:ident, $at:ident; $( $record:ident $name:literal { $($fields:expr;)* } )*) => {
        $(impl Body for $record {
            #[allow(unused_variables)]
            fn emit(&self, $w: &mut Writer, $at: Site) {
                let $n = self;
                let $at = Site {owner: Some(NodeTag::$record), ..$at};
                $($fields;)*
            }
        })*
        impl Body for Node {
            fn emit(&self, w: &mut Writer, at: Site) {
                match self {
                    Self::AConst(_) | Self::Alias(_) | Self::ParamRef(_) | Self::SetToDefault(_)
                        | Self::IntList(_) | Self::OidList(_) | Self::XidList(_) | Self::Null => return,
                    Self::TypeCast(n) if matches!(n.arg.as_deref(), Some(Node::AConst(_) | Node::ParamRef(_))) => return,
                    _ => {}
                }
                match self {
                    $(Self::$record(n) => { w.token($name); n.emit(w, at); },)*
                    Self::Integer(n) if *n != 0 => { w.token("Integer"); w.integer("ival", *n); }
                    Self::Float(n) => { w.token("Float"); w.string("str", Some(n)); }
                    Self::String(n) => { w.token("String"); w.string("str", Some(n)); }
                    Self::BitString(n) => { w.token("BitString"); w.string("str", Some(n)); }
                    Self::Boolean(n) => { w.token("Boolean"); w.token("boolval"); w.token(if *n {"true"} else {"false"}); }
                    Self::List(n) => w.list(n, at),
                    Self::AConst(_) | Self::Integer(_) | Self::IntList(_) | Self::OidList(_)
                        | Self::XidList(_) | Self::Null => {}
                }
            }
        }
    }
}

include!("records.in");

// [spec:pgq:req:fingerprint.digest]
pub fn fingerprint_nodes(nodes: &[Node], options: FingerprintOptions) -> Fingerprint {
    let mut w = Writer::new(options);
    w.list(nodes, Site::default());
    Fingerprint(xxh3_64_with_seed(&w.stream, FINGERPRINT_VERSION))
}

pub fn fingerprint_statements(stmts: &[RawStmt], options: FingerprintOptions) -> Fingerprint {
    let mut w = Writer::new(options);
    for stmt in stmts {
        w.token("RawStmt");
        stmt.emit(&mut w, Site::default().item());
    }
    Fingerprint(xxh3_64_with_seed(&w.stream, FINGERPRINT_VERSION))
}

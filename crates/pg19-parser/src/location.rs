//! Locations reported by the raw-node cases in PostgreSQL nodes/nodeFuncs.c.
use crate::raw::{Location, Node};

const UNKNOWN: Location = Location::UNKNOWN;

fn leftmost(a: Location, b: Location) -> Location {
    match (a.byte_offset(), b.byte_offset()) {
        (Some(x), Some(y)) if x > y => b,
        (Some(_), _) => a,
        _ => b,
    }
}

enum Source<'a> {
    Leaf(Location),
    Child(Location, Option<&'a Node>),
    List(Location, &'a [Node]),
}

fn leaf_location(node: &Node) -> Location {
    match node {
        Node::RangeVar(n) => n.location,
        Node::TableFunc(n) => n.location,
        Node::GroupingFunc(n) => n.location,
        Node::MergeSupportFunc(n) => n.location,
        Node::CaseExpr(n) => n.location,
        Node::CaseWhen(n) => n.location,
        Node::RowExpr(n) => n.location,
        Node::CoalesceExpr(n) => n.location,
        Node::MinMaxExpr(n) => n.location,
        Node::SQLValueFunction(n) => n.location,
        Node::JsonFormat(n) => n.location,
        Node::JsonIsPredicate(n) => n.location,
        Node::SetToDefault(n) => n.location,
        Node::ColumnRef(n) => n.location,
        Node::ParamRef(n) => n.location,
        Node::AConst(n) => n.location,
        Node::AArrayExpr(n) => n.location,
        Node::ResTarget(n) => n.location,
        Node::WindowDef(n) => n.location,
        Node::RangeTableSample(n) => n.location,
        Node::TypeName(n) => n.location,
        Node::ColumnDef(n) => n.location,
        Node::IndexElem(n) => n.location,
        Node::Constraint(n) => n.location,
        Node::FunctionParameter(n) => n.location,
        Node::XmlSerialize(n) => n.location,
        Node::GroupingSet(n) => n.location,
        Node::WithClause(n) => n.location,
        Node::InferClause(n) => n.location,
        Node::OnConflictClause(n) => n.location,
        Node::CTESearchClause(n) => n.location,
        Node::CTECycleClause(n) => n.location,
        Node::CommonTableExpr(n) => n.location,
        Node::JsonObjectConstructor(n) => n.location,
        Node::JsonArrayConstructor(n) => n.location,
        Node::JsonArrayQueryConstructor(n) => n.location,
        Node::JsonAggConstructor(n) => n.location,
        Node::PartitionElem(n) => n.location,
        Node::PartitionSpec(n) => n.location,
        Node::PartitionBoundSpec(n) => n.location,
        Node::PartitionRangeDatum(n) => n.location,
        _ => UNKNOWN,
    }
}

fn source(node: &Node) -> Source<'_> {
    use Source::*;
    match node {
        Node::FuncExpr(n) => List(n.location, &n.args),
        Node::NamedArgExpr(n) => Child(n.location, n.arg.as_deref()),
        Node::BoolExpr(n) => List(n.location, &n.args),
        Node::SubLink(n) => Child(n.location, n.testexpr.as_deref()),
        Node::XmlExpr(n) => List(n.location, &n.args),
        Node::JsonValueExpr(n) => Child(UNKNOWN, n.raw_expr.as_deref()),
        Node::JsonBehavior(n) => Child(UNKNOWN, n.expr.as_deref()),
        Node::NullTest(n) => Child(n.location, n.arg.as_deref()),
        Node::BooleanTest(n) => Child(n.location, n.arg.as_deref()),
        Node::IntoClause(n) => Leaf(n.rel.as_ref().map_or(UNKNOWN, |v| v.location)),
        Node::AExpr(n) => Child(n.location, n.lexpr.as_deref()),
        Node::FuncCall(n) => List(n.location, &n.args),
        Node::MultiAssignRef(n) => Child(UNKNOWN, n.source.as_deref()),
        Node::TypeCast(n) => Child(
            leftmost(
                n.location,
                n.type_name.as_ref().map_or(UNKNOWN, |t| t.location),
            ),
            n.arg.as_deref(),
        ),
        Node::CollateClause(n) => Child(UNKNOWN, n.arg.as_deref()),
        Node::SortBy(n) => Child(UNKNOWN, n.node.as_deref()),
        Node::JsonKeyValue(n) => Child(UNKNOWN, n.key.as_deref()),
        Node::JsonObjectAgg(n) => Leaf(n.constructor.as_ref().map_or(UNKNOWN, |v| v.location)),
        Node::JsonArrayAgg(n) => Leaf(n.constructor.as_ref().map_or(UNKNOWN, |v| v.location)),
        Node::List(n) => List(UNKNOWN, n),

        _ => Leaf(leaf_location(node)),
    }
}

/// exprLocation takes the first located member of a list, including nested lists.
/// Save pending lists on the heap so a deeply nested expression stays stack safe.
pub(super) fn expression_location(nodes: &[Node]) -> Location {
    let mut pending = Vec::new();
    let mut source = Source::List(UNKNOWN, nodes);
    let mut position = UNKNOWN;
    loop {
        match source {
            Source::Leaf(loc) => position = leftmost(position, loc),
            Source::Child(loc, child) => {
                position = leftmost(position, loc);
                if let Some(node) = child {
                    source = self::source(node);
                    continue;
                }
            }
            Source::List(loc, list) => {
                position = leftmost(position, loc);
                if let Some((head, tail)) = list.split_first() {
                    pending.push((position, tail));
                    source = self::source(head);
                    position = UNKNOWN;
                    continue;
                }
            }
        }
        loop {
            let Some((outer, remaining)) = pending.pop() else {
                return position;
            };
            if position == UNKNOWN {
                if let Some((head, tail)) = remaining.split_first() {
                    pending.push((outer, tail));
                    source = self::source(head);
                    break;
                }
            }
            position = leftmost(outer, position);
        }
    }
}

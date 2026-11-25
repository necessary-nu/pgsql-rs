//! Locations reported by the raw-node cases in PostgreSQL nodes/nodeFuncs.c.
use crate::raw::{Arena, Location, Node};

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

fn leaf_location(node: &Node, arena: &Arena) -> Location {
    match node {
        Node::RangeVar(n) => n.get(arena).location,
        Node::TableFunc(n) => n.get(arena).location,
        Node::GroupingFunc(n) => n.get(arena).location,
        Node::MergeSupportFunc(n) => n.get(arena).location,
        Node::CaseExpr(n) => n.get(arena).location,
        Node::CaseWhen(n) => n.get(arena).location,
        Node::RowExpr(n) => n.get(arena).location,
        Node::CoalesceExpr(n) => n.get(arena).location,
        Node::MinMaxExpr(n) => n.get(arena).location,
        Node::SQLValueFunction(n) => n.get(arena).location,
        Node::JsonFormat(n) => n.get(arena).location,
        Node::JsonIsPredicate(n) => n.get(arena).location,
        Node::SetToDefault(n) => n.get(arena).location,
        Node::ColumnRef(n) => n.get(arena).location,
        Node::ParamRef(n) => n.get(arena).location,
        Node::AConst(n) => n.location,
        Node::AArrayExpr(n) => n.get(arena).location,
        Node::ResTarget(n) => n.get(arena).location,
        Node::WindowDef(n) => n.get(arena).location,
        Node::RangeTableSample(n) => n.get(arena).location,
        Node::TypeName(n) => n.get(arena).location,
        Node::ColumnDef(n) => n.get(arena).location,
        Node::IndexElem(n) => n.get(arena).location,
        Node::Constraint(n) => n.get(arena).location,
        Node::FunctionParameter(n) => n.get(arena).location,
        Node::XmlSerialize(n) => n.get(arena).location,
        Node::GroupingSet(n) => n.get(arena).location,
        Node::WithClause(n) => n.get(arena).location,
        Node::InferClause(n) => n.get(arena).location,
        Node::OnConflictClause(n) => n.get(arena).location,
        Node::CTESearchClause(n) => n.get(arena).location,
        Node::CTECycleClause(n) => n.get(arena).location,
        Node::CommonTableExpr(n) => n.get(arena).location,
        Node::JsonObjectConstructor(n) => n.get(arena).location,
        Node::JsonArrayConstructor(n) => n.get(arena).location,
        Node::JsonArrayQueryConstructor(n) => n.get(arena).location,
        Node::JsonAggConstructor(n) => n.get(arena).location,
        Node::PartitionElem(n) => n.get(arena).location,
        Node::PartitionSpec(n) => n.get(arena).location,
        Node::PartitionBoundSpec(n) => n.get(arena).location,
        Node::PartitionRangeDatum(n) => n.get(arena).location,
        _ => UNKNOWN,
    }
}

fn source<'a>(node: &'a Node, arena: &'a Arena) -> Source<'a> {
    use Source::*;
    match node {
        Node::FuncExpr(n) => List(n.get(arena).location, n.get(arena).args.slice(arena)),
        Node::NamedArgExpr(n) => Child(
            n.get(arena).location,
            n.get(arena).arg.map(|id| id.get(arena)),
        ),
        Node::BoolExpr(n) => List(n.get(arena).location, n.get(arena).args.slice(arena)),
        Node::SubLink(n) => Child(
            n.get(arena).location,
            n.get(arena).testexpr.map(|id| id.get(arena)),
        ),
        Node::XmlExpr(n) => List(n.get(arena).location, n.get(arena).args.slice(arena)),
        Node::JsonValueExpr(n) => Child(UNKNOWN, n.get(arena).raw_expr.map(|id| id.get(arena))),
        Node::JsonBehavior(n) => Child(UNKNOWN, n.get(arena).expr.map(|id| id.get(arena))),
        Node::NullTest(n) => Child(
            n.get(arena).location,
            n.get(arena).arg.map(|id| id.get(arena)),
        ),
        Node::BooleanTest(n) => Child(
            n.get(arena).location,
            n.get(arena).arg.map(|id| id.get(arena)),
        ),
        Node::IntoClause(n) => Leaf(
            n.get(arena)
                .rel
                .as_ref()
                .map_or(UNKNOWN, |v| v.get(arena).location),
        ),
        Node::AExpr(n) => Child(
            n.get(arena).location,
            n.get(arena).lexpr.map(|id| id.get(arena)),
        ),
        Node::FuncCall(n) => List(n.get(arena).location, n.get(arena).args.slice(arena)),
        Node::MultiAssignRef(n) => Child(UNKNOWN, n.get(arena).source.map(|id| id.get(arena))),
        Node::TypeCast(n) => Child(
            leftmost(
                n.get(arena).location,
                n.get(arena)
                    .type_name
                    .as_ref()
                    .map_or(UNKNOWN, |t| t.get(arena).location),
            ),
            n.get(arena).arg.map(|id| id.get(arena)),
        ),
        Node::CollateClause(n) => Child(UNKNOWN, n.get(arena).arg.map(|id| id.get(arena))),
        Node::SortBy(n) => Child(UNKNOWN, n.get(arena).node.map(|id| id.get(arena))),
        Node::JsonKeyValue(n) => Child(UNKNOWN, n.get(arena).key.map(|id| id.get(arena))),
        Node::JsonObjectAgg(n) => Leaf(
            n.get(arena)
                .constructor
                .as_ref()
                .map_or(UNKNOWN, |v| v.get(arena).location),
        ),
        Node::JsonArrayAgg(n) => Leaf(
            n.get(arena)
                .constructor
                .as_ref()
                .map_or(UNKNOWN, |v| v.get(arena).location),
        ),
        Node::List(n) => List(UNKNOWN, n.slice(arena)),

        _ => Leaf(leaf_location(node, arena)),
    }
}

/// exprLocation takes the first located member of a list, including nested lists.
/// Save pending lists on the heap so a deeply nested expression stays stack safe.
pub(super) fn expression_location(nodes: &[Node], arena: &Arena) -> Location {
    let mut pending = Vec::new();
    let mut source = Source::List(UNKNOWN, nodes);
    let mut position = UNKNOWN;
    loop {
        match source {
            Source::Leaf(loc) => position = leftmost(position, loc),
            Source::Child(loc, child) => {
                position = leftmost(position, loc);
                if let Some(node) = child {
                    source = self::source(node, arena);
                    continue;
                }
            }
            Source::List(loc, list) => {
                position = leftmost(position, loc);
                if let Some((head, tail)) = list.split_first() {
                    pending.push((position, tail));
                    source = self::source(head, arena);
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
                    source = self::source(head, arena);
                    break;
                }
            }
            position = leftmost(outer, position);
        }
    }
}

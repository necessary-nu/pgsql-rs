use crate::*;

pub use protobuf::node::Node as NodeEnum;
use std::collections::VecDeque;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, serde::Serialize)]
pub enum Context {
    None,
    Select,
    DML,
    DDL,
    Call,
}

impl NodeEnum {
    pub fn deparse(&self, options: DeparseOptions) -> Result<String> {
        self.to_ref().deparse(options)
    }

    pub fn nodes(&self) -> Vec<(NodeRef<'_>, i32, Context, bool)> {
        let mut pending = VecDeque::from([(self.to_ref(), 1, Context::None, false)]);
        let mut nodes = Vec::new();
        while let Some((node, depth, context, filter)) = pending.pop_front() {
            nodes.push((node, depth, context, filter));
            node.legacy_children(context, filter, |child, context, filter| {
                pending.push_back((child, depth + 1, context, filter));
            });
        }
        nodes
    }

    /// # Safety
    /// The returned pointers borrow this tree. Dereferencing them requires live
    /// records, stable addresses, and exclusive access for each mutation.
    pub unsafe fn nodes_mut(&mut self) -> Vec<(NodeMut, i32, Context)> {
        let mut pending = VecDeque::from([(self.to_mut(), 1, Context::None)]);
        let mut nodes = Vec::new();
        while let Some((node, depth, context)) = pending.pop_front() {
            nodes.push((node, depth, context));
            node.borrow_exclusive()
                .legacy_children(context, false, |child, context, _| {
                    pending.push_back((child.into_raw(), depth + 1, context));
                });
        }
        nodes
    }
}

// pg_query.rs exposes a breadth-first projection with a different set of
// edges for each borrow direction. The descriptor visitor covers the full tree.
macro_rules! shared_only {
    (shared, $body:block) => {
        $body
    };
    (exclusive, $body:block) => {};
}

macro_rules! legacy_walk {
    ($view:ident, $direction:ident, $iter:ident, $borrow:ident, $convert:ident) => {
        impl<'a> $view<'a> {
            fn legacy_children(
                self,
                context: Context,
                filter: bool,
                mut push: impl FnMut(Self, Context, bool),
            ) {
                use Context::{Call, Select, DDL, DML};
                macro_rules! nodes {
                    ($field:expr, $ctx:expr, $filter:expr) => {
                        for item in $field.$iter() {
                            if let Some(child) = item.node.$borrow() {
                                push(child.$convert(), $ctx, $filter);
                            }
                        }
                    };
                }
                macro_rules! record {
                    ($kind:ident, $field:expr, $ctx:expr) => {
                        if let Some(child) = $field.$borrow() {
                            push(Self::$kind(child), $ctx, false);
                        }
                    };
                }
                macro_rules! ctes {
                    ($stmt:ident, $ctx:expr) => {
                        if let Some(with) = $stmt.with_clause.$borrow() {
                            nodes!(with.ctes, $ctx, false);
                        }
                    };
                }
                match self {
                    Self::SelectStmt(s) => {
                        nodes!(s.target_list, Select, false);
                        nodes!(s.where_clause, Select, true);
                        nodes!(s.sort_clause, Select, false);
                        nodes!(s.group_clause, Select, false);
                        nodes!(s.having_clause, Select, false);
                        ctes!(s, Select);
                        use protobuf::SetOperation::*;
                        match protobuf::SetOperation::try_from(s.op) {
                            Ok(SetopNone) => {
                                nodes!(s.from_clause, Select, false);
                            }
                            Ok(SetopUnion | SetopExcept | SetopIntersect) => {
                                record!(SelectStmt, s.larg, Select);
                                record!(SelectStmt, s.rarg, Select);
                            }
                            _ => {}
                        }
                    }
                    Self::InsertStmt(s) => {
                        nodes!(s.select_stmt, DML, false);
                        record!(RangeVar, s.relation, DML);
                        ctes!(s, DML);
                        record!(OnConflictClause, s.on_conflict_clause, DML);
                    }
                    Self::UpdateStmt(s) => {
                        nodes!(s.target_list, DML, false);
                        nodes!(s.where_clause, DML, true);
                        nodes!(s.from_clause, Select, false);
                        record!(RangeVar, s.relation, DML);
                        ctes!(s, DML);
                    }
                    Self::DeleteStmt(s) => {
                        nodes!(s.where_clause, DML, true);
                        record!(RangeVar, s.relation, DML);
                        ctes!(s, DML);
                        nodes!(s.using_clause, Select, false);
                    }
                    Self::MergeStmt(_s) => {
                        shared_only!($direction, {
                            record!(RangeVar, _s.relation, DML);
                            ctes!(_s, DML);
                            nodes!(_s.source_relation, Select, false);
                            nodes!(_s.merge_when_clauses, DML, true);
                            nodes!(_s.join_condition, Select, false);
                        });
                    }
                    Self::CommonTableExpr(s) => {
                        nodes!(s.ctequery, context, false);
                    }
                    Self::CopyStmt(s) => {
                        nodes!(s.query, DML, false);
                        record!(RangeVar, s.relation, DML);
                    }
                    Self::CallStmt(s) => {
                        record!(FuncCall, s.funccall, Call);
                    }
                    Self::AlterTableStmt(s) => {
                        record!(RangeVar, s.relation, DDL);
                    }
                    Self::CreateStmt(s) => {
                        record!(RangeVar, s.relation, DDL);
                    }
                    Self::CreateTableAsStmt(s) => {
                        nodes!(s.query, DDL, false);
                        if let Some(into) = s.into.$borrow() {
                            record!(RangeVar, into.rel, DDL);
                        }
                    }
                    Self::TruncateStmt(s) => {
                        nodes!(s.relations, DDL, false);
                    }
                    Self::ViewStmt(s) => {
                        nodes!(s.query, DDL, false);
                        record!(RangeVar, s.view, DDL);
                    }
                    Self::IndexStmt(s) => {
                        record!(RangeVar, s.relation, DDL);
                        for item in s.index_params.$iter() {
                            if let Some(NodeEnum::IndexElem(elem)) = item.node.$borrow() {
                                shared_only!($direction, {
                                    push(Self::IndexElem(elem), DDL, false);
                                });
                                nodes!(elem.expr, DDL, false);
                            }
                        }
                        shared_only!($direction, {
                            nodes!(s.where_clause, DDL, true);
                        });
                    }
                    Self::CreateTrigStmt(s) => {
                        record!(RangeVar, s.relation, DDL);
                    }
                    Self::RuleStmt(s) => {
                        record!(RangeVar, s.relation, DDL);
                    }
                    Self::RepackStmt(s) => {
                        if let Some(vacuum) = s.relation.$borrow() {
                            record!(RangeVar, vacuum.relation, DDL);
                        }
                    }
                    Self::VacuumStmt(s) => {
                        for item in s.rels.$iter() {
                            if let Some(NodeEnum::VacuumRelation(rel)) = item.node.$borrow() {
                                record!(RangeVar, rel.relation, DDL);
                            }
                        }
                    }
                    Self::RefreshMatViewStmt(s) => {
                        record!(RangeVar, s.relation, DDL);
                    }
                    Self::GrantStmt(s) if s.objtype == protobuf::ObjectType::ObjectTable as i32 => {
                        nodes!(s.objects, DDL, false);
                    }
                    Self::LockStmt(s) => {
                        nodes!(s.relations, DDL, false);
                    }
                    Self::ExplainStmt(s) => {
                        nodes!(s.query, context, false);
                    }
                    Self::AExpr(e) => {
                        nodes!(e.lexpr, context, filter);
                        nodes!(e.rexpr, context, filter);
                    }
                    Self::BoolExpr(e) => {
                        nodes!(e.args, context, filter);
                    }
                    Self::BooleanTest(_e) => {
                        shared_only!($direction, {
                            nodes!(_e.arg, context, filter);
                        });
                    }
                    Self::CoalesceExpr(e) => {
                        nodes!(e.args, context, filter);
                    }
                    Self::MinMaxExpr(e) => {
                        nodes!(e.args, context, filter);
                    }
                    Self::NullTest(e) => {
                        nodes!(e.arg, context, filter);
                    }
                    Self::ResTarget(e) => {
                        nodes!(e.val, context, filter);
                    }
                    Self::SubLink(e) => {
                        nodes!(e.subselect, context, filter);
                    }
                    Self::FuncCall(e) => {
                        nodes!(e.args, context, filter);
                    }
                    Self::CaseExpr(e) => {
                        nodes!(e.args, context, filter);
                        nodes!(e.defresult, context, filter);
                    }
                    Self::CaseWhen(e) => {
                        nodes!(e.expr, context, filter);
                        nodes!(e.result, context, filter);
                    }
                    Self::SortBy(e) => {
                        nodes!(e.node, context, filter);
                    }
                    Self::TypeCast(e) => {
                        nodes!(e.arg, context, filter);
                    }
                    Self::List(e) => {
                        nodes!(e.items, context, filter);
                    }
                    Self::JoinExpr(e) => {
                        nodes!(e.larg, context, filter);
                        nodes!(e.rarg, context, filter);
                        nodes!(e.quals, context, filter);
                    }
                    Self::RowExpr(e) => {
                        nodes!(e.args, context, filter);
                    }
                    Self::RangeSubselect(e) => {
                        nodes!(e.subquery, context, filter);
                    }
                    Self::RangeFunction(e) => {
                        nodes!(e.functions, context, filter);
                    }
                    _ => {}
                }
            }
        }
    };
}
legacy_walk!(NodeRef, shared, iter, as_ref, to_ref);
legacy_walk!(NodeRefMut, exclusive, iter_mut, as_mut, borrow_mut);

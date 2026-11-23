use super::{children, Context, FilterColumn, Function, Summary, Table};
use crate::{deparse::quoted_identifier, raw_walk::RawRef};
use pg19_nodes::*;
use std::collections::HashSet;

struct Relation<'a> {
    name: &'a str,
    schema: Option<&'a str>,
    alias: Option<&'a str>,
    node: Option<*const RangeVar>,
    context: Context,
}

#[derive(Default)]
pub(super) struct State<'a> {
    pub(super) summary: Summary,
    ranges: Vec<Relation<'a>>,
    names: HashSet<&'a str>,
    own_ranges: HashSet<*const RangeVar>,
}

fn text(node: &Node) -> Option<&str> {
    if let Node::String(s) = node {
        Some(s)
    } else {
        None
    }
}

impl<'a> State<'a> {
    pub(super) fn finish(mut self) -> Summary {
        for r in self.ranges {
            let name = match r.schema {
                Some(schema) => format!(
                    "{}.{}",
                    quoted_identifier(schema),
                    quoted_identifier(r.name)
                ),
                None => quoted_identifier(r.name),
            };
            if let Some(alias) = r.alias {
                self.summary.aliases.insert(alias.to_owned(), name.clone());
            }
            let cte = r.context == Context::Select
                && r.schema.is_none()
                && self.names.contains(r.name)
                && !r.node.is_some_and(|ptr| self.own_ranges.contains(&ptr));
            if !cte {
                self.summary.tables.push(Table {
                    name,
                    schema_name: r.schema.unwrap_or_default().into(),
                    table_name: r.name.into(),
                    context: r.context as i32,
                });
            }
        }
        self.summary
    }

    pub(super) fn range(&mut self, r: Option<&'a RangeVar>, context: Context) {
        if let Some(r) = r {
            self.ranges.push(Relation {
                name: r.relname.as_deref().unwrap_or_default(),
                schema: r.schemaname.as_deref(),
                alias: r.alias.as_ref().and_then(|a| a.aliasname.as_deref()),
                node: Some(r),
                context,
            });
        }
    }

    pub(super) fn ranges(&mut self, items: &'a [Node], context: Context) {
        let mut todo: Vec<_> = items.iter().rev().collect();
        while let Some(item) = todo.pop() {
            match item {
                Node::RangeVar(r) => self.range(Some(r), context),
                Node::JoinExpr(j) => {
                    todo.extend(j.rarg.as_deref());
                    todo.extend(j.larg.as_deref());
                }
                Node::RangeTableSample(s) => todo.extend(s.relation.as_deref()),
                _ => {}
            }
        }
    }

    // [spec:pgq:req:summary.cte-scope]
    fn with_clause(&mut self, clause: Option<&'a WithClause>) {
        let Some(clause) = clause else { return };
        for n in &clause.ctes {
            let Node::CommonTableExpr(cte) = n else {
                continue;
            };
            let Some(name) = cte.ctename.as_deref() else {
                continue;
            };
            if !clause.recursive && !self.names.contains(name) {
                let mut jobs: Vec<_> = cte
                    .ctequery
                    .as_deref()
                    .into_iter()
                    .map(RawRef::from)
                    .collect();
                while let Some(r) = jobs.pop() {
                    if let RawRef::RangeVar(v) = r {
                        if v.schemaname.is_none() && v.relname.as_deref() == Some(name) {
                            self.own_ranges.insert(v);
                        }
                    }
                    children(r, &mut jobs);
                }
            }
            self.names.insert(name);
            self.summary.cte_names.push(name.to_owned());
        }
    }

    pub(super) fn function(&mut self, parts: &[Node], context: Context) {
        let names: Vec<_> = parts.iter().filter_map(text).collect();
        let Some(&last) = names.last() else { return };
        let schema = match names.as_slice() {
            [_, s, _] | [s, _] => *s,
            _ => "",
        };
        self.summary.functions.push(Function {
            name: names.join("."),
            schema_name: schema.into(),
            function_name: last.into(),
            context: context as i32,
        });
    }

    pub(super) fn column(&mut self, c: &ColumnRef) {
        if let Some(column) = c.fields.last().and_then(text) {
            let (schema, table) = match c.fields.as_slice() {
                [s, t, _] => (text(s), text(t)),
                [t, _] => (None, text(t)),
                _ => (None, None),
            };
            self.summary.filter_columns.push(FilterColumn {
                schema_name: schema.unwrap_or_default().into(),
                table_name: table.unwrap_or_default().into(),
                column: column.into(),
            });
        }
    }

    // [spec:pgq:req:summary.nested-filters]
    fn filters(&mut self, root: &'a Node) {
        let mut jobs = vec![root.into()];
        while let Some(r) = jobs.pop() {
            match r {
                RawRef::SelectStmt(_) => break,
                RawRef::FuncCall(f) => self.function(&f.funcname, Context::Call),
                RawRef::ColumnRef(c) => {
                    self.column(c);
                }
                _ => {}
            }
            children(r, &mut jobs);
        }
    }

    pub(super) fn walk(&mut self, nodes: &'a [Node]) {
        let mut jobs = vec![RawRef::List(nodes)];
        while let Some(r) = jobs.pop() {
            children(r, &mut jobs);
            match r {
                RawRef::RawStmt(n) => jobs.extend(n.stmt.as_deref().map(RawRef::from)),
                RawRef::CallStmt(n) => jobs.extend(n.funccall.as_deref().map(RawRef::from)),
                RawRef::FuncCall(n) => self.function(&n.funcname, Context::Call),
                RawRef::SelectStmt(n) => {
                    self.with_clause(n.with_clause.as_deref());
                    if n.op == SetOperation::SETOP_NONE {
                        self.ranges(&n.from_clause, Context::Select);
                    }
                    self.range(
                        n.into_clause.as_ref().and_then(|n| n.rel.as_deref()),
                        Context::Ddl,
                    );
                    if let Some(w) = n.where_clause.as_deref() {
                        self.filters(w);
                    }
                }
                RawRef::InsertStmt(n) => {
                    self.with_clause(n.with_clause.as_deref());
                    self.range(n.relation.as_deref(), Context::Dml);
                }
                RawRef::MergeStmt(n) => {
                    self.with_clause(n.with_clause.as_deref());
                    self.range(n.relation.as_deref(), Context::Dml);
                }
                RawRef::UpdateStmt(n) => {
                    self.with_clause(n.with_clause.as_deref());
                    self.range(n.relation.as_deref(), Context::Dml);
                    self.ranges(&n.from_clause, Context::Select);
                }
                RawRef::DeleteStmt(n) => {
                    self.with_clause(n.with_clause.as_deref());
                    self.range(n.relation.as_deref(), Context::Dml);
                    self.ranges(&n.using_clause, Context::Select);
                }
                RawRef::CopyStmt(n) => {
                    self.range(n.relation.as_deref(), Context::Select);
                    jobs.extend(n.query.as_deref().map(RawRef::from));
                }
                _ => {
                    self.utility(r);
                    match r {
                        RawRef::CreateTableAsStmt(n) => {
                            jobs.extend(n.query.as_deref().map(RawRef::from))
                        }
                        RawRef::ViewStmt(n) => jobs.extend(n.query.as_deref().map(RawRef::from)),
                        RawRef::ExplainStmt(n) => jobs.extend(n.query.as_deref().map(RawRef::from)),
                        RawRef::PrepareStmt(n) => jobs.extend(n.query.as_deref().map(RawRef::from)),
                        RawRef::IndexStmt(n) => {
                            jobs.extend(n.where_clause.as_deref().map(RawRef::from));
                            for elem in n.index_params.iter().rev() {
                                if let Node::IndexElem(i) = elem {
                                    jobs.extend(i.expr.as_deref().map(RawRef::from));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub(super) fn utility(&mut self, r: RawRef<'a>) {
        let ddl = Context::Ddl;
        match r {
            RawRef::AlterTableStmt(n) if n.objtype != ObjectType::OBJECT_INDEX => {
                self.range(n.relation.as_deref(), ddl)
            }
            RawRef::CreateStmt(n) => self.range(n.relation.as_deref(), ddl),
            RawRef::CreateTableAsStmt(n) => {
                self.range(n.into.as_ref().and_then(|i| i.rel.as_deref()), ddl);
            }
            RawRef::TruncateStmt(n) => self.ranges(&n.relations, ddl),
            RawRef::ViewStmt(n) => {
                self.range(n.view.as_deref(), ddl);
            }
            RawRef::IndexStmt(n) => {
                self.range(n.relation.as_deref(), ddl);
            }
            RawRef::CreateTrigStmt(n) => self.range(n.relation.as_deref(), ddl),
            RawRef::RuleStmt(n) => self.range(n.relation.as_deref(), ddl),
            RawRef::VacuumStmt(n) => {
                for rel in &n.rels {
                    if let Node::VacuumRelation(r) = rel {
                        self.range(r.relation.as_deref(), ddl);
                    }
                }
            }
            RawRef::RefreshMatViewStmt(n) => self.range(n.relation.as_deref(), ddl),
            RawRef::LockStmt(n) => self.ranges(&n.relations, ddl),
            RawRef::CreateFunctionStmt(n) => self.function(&n.funcname, ddl),
            RawRef::DropStmt(n) => self.drop_objects(n),
            RawRef::RenameStmt(n) => self.rename(n),
            RawRef::GrantStmt(n) => self.grant(n),
            _ => {}
        }
    }

    fn named_range(&mut self, name: &'a str, schema: Option<&'a str>) {
        self.ranges.push(Relation {
            name,
            schema,
            alias: None,
            node: None,
            context: Context::Ddl,
        });
    }

    fn drop_objects(&mut self, n: &'a DropStmt) {
        use pg19_nodes::ObjectType as O;
        match n.remove_type {
            O::OBJECT_TABLE | O::OBJECT_RULE | O::OBJECT_TRIGGER => {
                for obj in &n.objects {
                    let Node::List(names) = obj else { continue };
                    let parts = if n.remove_type == O::OBJECT_TABLE {
                        names.as_slice()
                    } else {
                        names.split_last().map_or(&[][..], |(_, prefix)| prefix)
                    };
                    if let Some((last, prefix)) = parts.split_last() {
                        if let Some(name) = text(last) {
                            self.named_range(name, prefix.last().and_then(text));
                        }
                    }
                }
            }
            O::OBJECT_FUNCTION => {
                if let Some(Node::ObjectWithArgs(obj)) = n.objects.first() {
                    self.function(&obj.objname, Context::Ddl);
                }
            }
            _ => {}
        }
    }

    fn rename(&mut self, n: &'a RenameStmt) {
        use pg19_nodes::ObjectType as O;
        let Some(newname) = n.newname.as_deref() else {
            return;
        };
        match n.rename_type {
            O::OBJECT_TABLE => {
                if let Some(r) = n.relation.as_deref() {
                    self.range(Some(r), Context::Ddl);
                    self.named_range(newname, r.schemaname.as_deref());
                }
            }
            O::OBJECT_FUNCTION | O::OBJECT_PROCEDURE | O::OBJECT_ROUTINE => {
                if let Some(Node::ObjectWithArgs(obj)) = n.object.as_deref() {
                    self.function(&obj.objname, Context::Ddl);
                }
                self.summary.functions.push(Function {
                    name: newname.into(),
                    function_name: newname.into(),
                    schema_name: String::new(),
                    context: Context::Ddl as i32,
                });
            }
            _ => {}
        }
    }

    fn grant(&mut self, n: &'a GrantStmt) {
        use pg19_nodes::ObjectType as O;
        for obj in &n.objects {
            match (n.objtype, obj) {
                (O::OBJECT_TABLE, Node::RangeVar(r)) => self.range(Some(r), Context::Ddl),
                (
                    O::OBJECT_FUNCTION | O::OBJECT_PROCEDURE | O::OBJECT_ROUTINE,
                    Node::ObjectWithArgs(a),
                ) => self.function(&a.objname, Context::Ddl),
                _ => {}
            }
        }
    }
}

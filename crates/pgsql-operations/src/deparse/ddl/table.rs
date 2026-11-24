use super::{
    ident, literal, named, names, relation, relation_list, required, Error, Plan, Result, Role,
};
use pgsql_nodes::*;

pub(super) fn persistence(r: &RangeVar, p: &mut Plan<'_>) -> Result<()> {
    match r.relpersistence {
        b't' => p.text("TEMPORARY "),
        b'u' => p.text("UNLOGGED "),
        b'p' => {}
        _ => return Err(Error::InvalidField("relation persistence")),
    }
    Ok(())
}
pub(super) fn generic_options(options: &[Node], p: &mut Plan<'_>) -> Result<()> {
    if options.is_empty() {
        return Ok(());
    }
    p.text(" OPTIONS (");
    for (i, n) in options.iter().enumerate() {
        if i > 0 {
            p.text(", ")
        }
        let Node::DefElem(d) = n else {
            return Err(Error::InvalidField("foreign options"));
        };
        match d.defaction {
            DefElemAction::DEFELEM_DROP => p.text("DROP "),
            DefElemAction::DEFELEM_ADD => p.text("ADD "),
            DefElemAction::DEFELEM_SET => p.text("SET "),
            DefElemAction::DEFELEM_UNSPEC => {}
            _ => return Err(Error::InvalidField("DefElem.defaction")),
        }
        p.text(named(&d.defname, "DefElem.defname")?);
        if d.defaction != DefElemAction::DEFELEM_DROP {
            let Some(Node::String(v)) = d.arg.as_deref() else {
                return Err(Error::InvalidField("foreign option value"));
            };
            p.text(" ");
            p.text(literal(v)?);
        }
    }
    p.text(")");
    Ok(())
}

pub(super) fn column<'a>(s: &'a ColumnDef, p: &mut Plan<'a>) -> Result<()> {
    p.mark(s.location);
    p.text(named(&s.colname, "ColumnDef.colname")?);
    if let Some(t) = s.type_name.as_deref() {
        p.text(" ");
        p.typename(t)
    }
    if let Some(storage) = &s.storage_name {
        p.text(" STORAGE ");
        p.text(ident(storage)?)
    }
    if let Some(e) = s.raw_default.as_deref() {
        p.text(" USING ");
        p.expr(e)
    }
    if let Some(comp) = &s.compression {
        p.text(" COMPRESSION ");
        p.text(ident(comp)?)
    }
    generic_options(&s.fdwoptions, p)?;
    if let Some(c) = s.coll_clause.as_deref() {
        p.text(" COLLATE ");
        p.text(names(&c.collname, ".")?)
    }
    for c in &s.constraints {
        let Node::Constraint(c) = c else {
            return Err(Error::InvalidField("ColumnDef.constraints"));
        };
        p.text(" ");
        super::constraints::constraint(c, false, p)?;
    }
    Ok(())
}
fn table_like(s: &TableLikeClause, p: &mut Plan<'_>) -> Result<()> {
    p.text("LIKE ");
    relation(required(&s.relation, "TableLikeClause.relation")?, p, false)?;
    const FLAGS: [&str; 9] = [
        "COMMENTS",
        "COMPRESSION",
        "CONSTRAINTS",
        "DEFAULTS",
        "GENERATED",
        "IDENTITY",
        "INDEXES",
        "STATISTICS",
        "STORAGE",
    ];
    let all = i32::MAX as u32;
    if s.options & !all != 0 {
        return Err(Error::InvalidField("TableLikeClause.options"));
    }
    let base_all = s.options & !((1 << FLAGS.len()) - 1) != 0;
    if base_all {
        p.text(" INCLUDING ALL")
    }
    for (i, name) in FLAGS.iter().enumerate() {
        let set = s.options & (1 << i) != 0;
        if set != base_all {
            p.text(if set { " INCLUDING " } else { " EXCLUDING " });
            p.text(*name)
        }
    }
    Ok(())
}

pub(super) fn partition<'a>(s: &'a PartitionSpec, p: &mut Plan<'a>) -> Result<()> {
    p.mark(s.location);
    p.clause("PARTITION BY ");
    p.text(match s.strategy {
        PartitionStrategy::PARTITION_STRATEGY_RANGE => "RANGE (",
        PartitionStrategy::PARTITION_STRATEGY_LIST => "LIST (",
        PartitionStrategy::PARTITION_STRATEGY_HASH => "HASH (",
        _ => return Err(Error::InvalidField("PartitionSpec.strategy")),
    });
    for (i, n) in s.part_params.iter().enumerate() {
        if i > 0 {
            p.text(", ")
        }
        let Node::PartitionElem(e) = n else {
            return Err(Error::InvalidField("PartitionSpec.partParams"));
        };
        if let Some(name) = &e.name {
            p.text(ident(name)?)
        } else {
            p.wrapped(required(&e.expr, "PartitionElem.expr")?, Role::Expr, true)
        }
        if !e.collation.is_empty() {
            p.text(" COLLATE ");
            p.text(names(&e.collation, ".")?)
        }
        if !e.opclass.is_empty() {
            p.text(" ");
            p.text(names(&e.opclass, ".")?)
        }
    }
    p.text(")");
    Ok(())
}
pub(super) fn bound<'a>(s: &'a PartitionBoundSpec, p: &mut Plan<'a>) -> Result<()> {
    p.mark(s.location);
    if s.is_default {
        p.text("DEFAULT");
        return Ok(());
    }
    match s.strategy {
        b'h' => p.text(format!(
            "FOR VALUES WITH (MODULUS {}, REMAINDER {})",
            s.modulus, s.remainder
        )),
        b'l' => {
            p.text("FOR VALUES IN (");
            p.list(&s.listdatums, Role::Expr);
            p.text(")")
        }
        b'r' => {
            p.text("FOR VALUES FROM (");
            p.list(&s.lowerdatums, Role::Expr);
            p.text(") TO (");
            p.list(&s.upperdatums, Role::Expr);
            p.text(")")
        }
        _ => return Err(Error::InvalidField("PartitionBoundSpec.strategy")),
    }
    Ok(())
}

fn elements<'a>(nodes: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    if nodes.is_empty() {
        p.text(" ()");
        return Ok(());
    }
    p.text(" (");
    p.enter();
    for (i, n) in nodes.iter().enumerate() {
        if i > 0 {
            p.comma()
        }
        match n {
            Node::ColumnDef(c) => column(c, p)?,
            Node::Constraint(c) => super::constraints::constraint(c, false, p)?,
            Node::TableLikeClause(t) => table_like(t, p)?,
            _ => return Err(Error::InvalidField("CreateStmt.tableElts")),
        }
    }
    p.leave();
    p.text(")");
    Ok(())
}

pub(super) fn create<'a>(s: &'a CreateStmt, foreign: bool, p: &mut Plan<'a>) -> Result<()> {
    let r = required(&s.relation, "CreateStmt.relation")?;
    p.text("CREATE ");
    if foreign {
        p.text("FOREIGN ")
    }
    persistence(r, p)?;
    p.text("TABLE ");
    if s.if_not_exists {
        p.text("IF NOT EXISTS ")
    }
    relation(r, p, false)?;
    if let Some(t) = s.of_typename.as_deref() {
        p.text(" OF ");
        p.typename(t)
    }
    if s.partbound.is_some() {
        let [Node::RangeVar(parent)] = s.inh_relations.as_slice() else {
            return Err(Error::InvalidField("partition parent"));
        };
        p.text(" PARTITION OF ");
        relation(parent, p, false)?
    }
    if !s.table_elts.is_empty() || (s.partbound.is_none() && s.of_typename.is_none()) {
        elements(&s.table_elts, p)?
    }
    if let Some(b) = s.partbound.as_deref() {
        p.text(" ");
        bound(b, p)?
    } else if !s.inh_relations.is_empty() {
        p.text(" INHERITS (");
        relation_list(&s.inh_relations, p)?;
        p.text(")")
    }
    if let Some(z) = s.partspec.as_deref() {
        partition(z, p)?
    }
    if let Some(am) = &s.access_method {
        p.text(" USING ");
        p.text(ident(am)?)
    }
    if !s.options.is_empty() {
        p.text(" WITH ");
        super::index::reloptions(&s.options, p)?;
    }
    match s.oncommit {
        OnCommitAction::ONCOMMIT_NOOP => {}
        OnCommitAction::ONCOMMIT_DROP => p.text(" ON COMMIT DROP"),
        OnCommitAction::ONCOMMIT_DELETE_ROWS => p.text(" ON COMMIT DELETE ROWS"),
        OnCommitAction::ONCOMMIT_PRESERVE_ROWS => p.text(" ON COMMIT PRESERVE ROWS"),
        _ => return Err(Error::InvalidField("CreateStmt.oncommit")),
    };
    if let Some(ts) = &s.tablespacename {
        p.text(" TABLESPACE ");
        p.text(ident(ts)?)
    }
    Ok(())
}

pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::AlterTableStmt(s) => super::alter::alter(s, p)?,
        Node::CreateStmt(s) => create(s, false, p)?,
        Node::CreateForeignTableStmt(s) => {
            create(&s.base, true, p)?;
            p.text(" SERVER ");
            p.text(named(&s.servername, "CreateForeignTableStmt.servername")?);
            generic_options(&s.options, p)?;
        }
        _ => return super::definition::statement(n, p),
    }
    Ok(true)
}

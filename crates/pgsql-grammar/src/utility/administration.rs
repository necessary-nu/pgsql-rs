//! Administrative utility actions with validations in PostgreSQL gram.y.
use crate::raw::*;
use crate::{
    records::{optional_node, optional_text, record},
    value::Value,
    Failure,
};
use pgsql_engine::Reduction;

pub(crate) fn security_mode(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let name = rhs.take(2).text()?;
    let enabled = match name.as_str() {
        "permissive" => true,
        "restrictive" => false,
        _ => {
            return Err(Failure::At(
                format!("unrecognized row security option \"{name}\""),
                "42601",
                rhs.location(2),
            ))
        }
    };
    Ok(Value::Integer(i32::from(enabled)))
}

pub(crate) fn like_options(
    rhs: &mut Reduction<'_, Value>,
    include: bool,
) -> Result<Value, Failure> {
    let current = rhs.take(1).integer()?;
    let requested = rhs.take(3).integer()?;
    Ok(Value::Integer(if include {
        current | requested
    } else {
        current & !requested
    }))
}

pub(crate) fn statistics(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let column = rhs.take(3).integer()?;
    let num = i16::try_from(column)
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            Failure::At(
                format!("column number must be in range from 1 to {}", i16::MAX),
                "22023",
                rhs.location(3),
            )
        })?;
    Ok(Value::from_node(
        AlterTableCmd {
            subtype: AlterTableType::AT_SetStatistics,
            num,
            def: optional_node(rhs.take(6))?,
            ..Default::default()
        }
        .into(),
    ))
}

pub(crate) fn identity_option(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let node = rhs.take(2).node()?;
    let Node::DefElem(def) = &node else {
        return Err("expected identity sequence option".into());
    };
    if let Some(name @ ("as" | "restart" | "owned_by")) = def.defname.as_deref() {
        return Err(Failure::At(
            format!("sequence option \"{name}\" not supported here"),
            "42601",
            rhs.location(2),
        ));
    }
    Ok(Value::from_node(node))
}

pub(crate) fn import_qualification(
    rhs: &mut Reduction<'_, Value>,
    qualified: bool,
) -> Result<Value, Failure> {
    let (list_type, table_list) = if qualified {
        (
            ImportForeignSchemaType(rhs.take(1).integer()?),
            rhs.take(3).list()?,
        )
    } else {
        (ImportForeignSchemaType::FDW_IMPORT_SCHEMA_ALL, Vec::new())
    };
    Ok(Value::from_node(
        ImportForeignSchemaStmt {
            list_type,
            table_list,
            ..Default::default()
        }
        .into(),
    ))
}

pub(crate) fn import_schema(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let Node::ImportForeignSchemaStmt(mut stmt) = rhs.take(5).node()? else {
        return Err("expected import qualification".into());
    };
    stmt.remote_schema = Some(rhs.take(4).text()?);
    stmt.server_name = Some(rhs.take(8).text()?);
    stmt.local_schema = Some(rhs.take(10).text()?);
    stmt.options = rhs.take(11).list()?;
    Ok(Value::from_node(Node::ImportForeignSchemaStmt(stmt)))
}

pub(crate) fn revoke_role_option(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let opt = DefElem {
        defname: Some(rhs.take(2).text()?),
        arg: Some(Box::new(Node::Boolean(false))),
        location: rhs.location(2),
        arg_location: Location::UNKNOWN,
        ..Default::default()
    };
    Ok(Value::from_node(
        GrantRoleStmt {
            is_grant: false,
            opt: vec![opt.into()],
            granted_roles: rhs.take(5).list()?,
            grantee_roles: rhs.take(7).list()?,
            grantor: record!(rhs.take(8), RoleSpec),
            behavior: DropBehavior(rhs.take(9).integer()?),
        }
        .into(),
    ))
}

pub(crate) fn set_optional(
    rhs: &mut Reduction<'_, Value>,
    timezone: bool,
) -> Result<Value, Failure> {
    let (name, location, arg) = if timezone {
        (
            "timezone",
            Location::UNKNOWN,
            optional_node(rhs.take(3))?.map(|v| v.into_inner()),
        )
    } else {
        let location = rhs.location(2);
        (
            "client_encoding",
            location,
            optional_text(rhs.take(2))?.map(|v| {
                Node::AConst(AConst {
                    val: Literal::String(v),
                    location,
                })
            }),
        )
    };
    let kind = if arg.is_some() {
        VariableSetKind::VAR_SET_VALUE
    } else {
        VariableSetKind::VAR_SET_DEFAULT
    };
    Ok(Value::from_node(
        VariableSetStmt {
            kind,
            name: Some(name.into()),
            args: arg.into_iter().collect(),
            location,
            jumble_args: timezone,
            ..Default::default()
        }
        .into(),
    ))
}

pub(crate) fn alter_constraint(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    use super::constraint::*;
    let bits = rhs.take(4).integer()?;
    if bits & CAS_NOT_VALID != 0 {
        return Err(Failure::At(
            "constraints cannot be altered to be NOT VALID".into(),
            "0A000",
            rhs.location(4),
        ));
    }
    let mut props = Constraint::default();
    process(
        &mut props,
        bits,
        rhs.location(4),
        "FOREIGN KEY",
        CAS_DEFERRABLE | CAS_INITIALLY_DEFERRED | CAS_ENFORCED | CAS_NO_INHERIT,
    )?;
    let changes = ATAlterConstraint {
        conname: Some(rhs.take(3).text()?),
        alter_enforceability: bits & (CAS_NOT_ENFORCED | CAS_ENFORCED) != 0,
        is_enforced: props.is_enforced,
        alter_deferrability: bits
            & (CAS_DEFERRABLE
                | CAS_NOT_DEFERRABLE
                | CAS_INITIALLY_DEFERRED
                | CAS_INITIALLY_IMMEDIATE)
            != 0,
        deferrable: props.deferrable,
        initdeferred: props.initdeferred,
        alter_inheritability: bits & CAS_NO_INHERIT != 0,
        noinherit: props.is_no_inherit,
    };
    Ok(Value::from_node(
        AlterTableCmd {
            subtype: AlterTableType::AT_AlterConstraint,
            def: Some(Box::new(changes.into())),
            ..Default::default()
        }
        .into(),
    ))
}

pub(crate) fn foreign_table(
    rhs: &mut Reduction<'_, Value>,
    if_not_exists: bool,
    partition: bool,
) -> Result<Value, Failure> {
    let rel = if if_not_exists { 7 } else { 4 };
    let Some(mut relation) = record!(rhs.take(rel), RangeVar) else {
        return Err("expected foreign relation".into());
    };
    relation.relpersistence = b'p';
    let (table_elts, inh_relations, partbound, server) = if partition {
        (
            rhs.take(rel + 4).list()?,
            vec![rhs.take(rel + 3).node()?],
            record!(rhs.take(rel + 5), PartitionBoundSpec),
            rel + 7,
        )
    } else {
        (
            rhs.take(rel + 2).list()?,
            rhs.take(rel + 4).list()?,
            None,
            rel + 6,
        )
    };
    let base = CreateStmt {
        relation: Some(relation),
        table_elts,
        inh_relations,
        partbound,
        if_not_exists,
        oncommit: OnCommitAction::ONCOMMIT_NOOP,
        ..Default::default()
    };
    Ok(Value::from_node(
        CreateForeignTableStmt {
            base,
            servername: Some(rhs.take(server).text()?),
            options: rhs.take(server + 1).list()?,
        }
        .into(),
    ))
}

//! CREATE TABLE AS, materialized views, and the recursive-view rewrite in gram.y.
use crate::raw::*;
use crate::{records::record, value::Value, Failure};
use pg19_engine::Reduction;

pub(crate) enum Source {
    Select,
    Execute,
    Materialized,
}

pub(super) fn create_as(
    rhs: &mut Reduction<'_, Value>,
    source: Source,
    if_not_exists: bool,
) -> Result<Value, Failure> {
    let materialized = matches!(source, Source::Materialized);
    let target = 4 + isize::from(materialized) + if if_not_exists { 3 } else { 0 };
    let mut into = record!(rhs.take(target), IntoClause).ok_or("expected INTO clause")?;
    into.rel
        .as_deref_mut()
        .ok_or("expected INTO relation")?
        .relpersistence = rhs.take(2).integer()? as u8;
    let query = match source {
        Source::Execute => {
            into.skip_data = rhs.take(target + 5).integer()? == 0;
            ExecuteStmt {
                name: Some(rhs.take(target + 3).text()?),
                params: rhs.take(target + 4).list()?,
            }
            .into()
        }
        Source::Select | Source::Materialized => {
            into.skip_data = rhs.take(target + 3).integer()? == 0;
            rhs.take(target + 2).node()?
        }
    };
    Ok(Value::from_node(
        CreateTableAsStmt {
            query: Some(Box::new(query)),
            into: Some(into),
            objtype: if materialized {
                ObjectType::OBJECT_MATVIEW
            } else {
                ObjectType::OBJECT_TABLE
            },
            is_select_into: false,
            if_not_exists,
        }
        .into(),
    ))
}

pub(super) fn recursive(rhs: &mut Reduction<'_, Value>, replace: bool) -> Result<Value, Failure> {
    let shift = if replace { 2 } else { 0 };
    let check = ViewCheckOption(rhs.take(12 + shift).integer()?);
    if check != ViewCheckOption::NO_CHECK_OPTION {
        return Err(Failure::At(
            "WITH CHECK OPTION not supported on recursive views".to_owned(),
            "0A000",
            rhs.location(12 + shift),
        ));
    }
    let mut view = record!(rhs.take(5 + shift), RangeVar).ok_or("expected view relation")?;
    view.relpersistence = rhs.take(2 + shift).integer()? as u8;
    let relname = view.relname.ok_or("expected view name")?;
    let aliases = rhs.take(7 + shift).list()?;
    let location = Location::UNKNOWN;
    let mut targets = Vec::with_capacity(aliases.len());
    for col in &aliases {
        let Node::String(name) = col else {
            return Err("expected view column name".into());
        };
        let column = ColumnRef {
            fields: vec![Node::String(*name)],
            location,
        };
        targets.push(
            ResTarget {
                val: Some(Box::new(column.into())),
                location,
                ..Default::default()
            }
            .into(),
        );
    }
    let cte = CommonTableExpr {
        ctename: Some(relname),
        aliascolnames: aliases.clone(),
        ctequery: rhs.take(11 + shift).optional()?,
        location,
        ..Default::default()
    };
    let from = super::range_name([relname], location, true)?;
    let query = SelectStmt {
        with_clause: Some(Box::new(WithClause {
            recursive: true,
            ctes: vec![cte.into()],
            location,
        })),
        target_list: targets,
        from_clause: vec![from.into()],
        ..Default::default()
    };
    Ok(Value::from_node(
        ViewStmt {
            view: Some(view),
            aliases,
            query: Some(Box::new(query.into())),
            replace,
            options: rhs.take(9 + shift).list()?,
            with_check_option: check,
        }
        .into(),
    ))
}

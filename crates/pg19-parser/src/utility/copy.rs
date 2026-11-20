use crate::raw::*;
use crate::{
    records::{optional_text, record},
    value::Value,
    Failure,
};
use pg19_engine::Reduction;

pub(super) fn copy(rhs: &mut Reduction<'_, Value>, query: bool) -> Result<Value, Failure> {
    let mut statement = CopyStmt {
        filename: optional_text(rhs.take(7))?,
        is_program: rhs.take(6).integer()? != 0,
        ..Default::default()
    };
    if statement.is_program && statement.filename.is_none() {
        return Err(Failure::At(
            "STDIN/STDOUT not allowed with PROGRAM".to_owned(),
            "42601",
            rhs.location(if query { 5 } else { 8 }),
        ));
    }
    if query {
        statement.query = rhs.take(3).optional()?;
        statement.options = rhs.take(9).list()?;
    } else {
        statement.is_from = rhs.take(5).integer()? != 0;
        statement.relation = record!(rhs.take(3), RangeVar);
        statement.attlist = rhs.take(4).list()?;
        statement.where_clause = rhs.take(11).optional()?;
        if !statement.is_from && statement.where_clause.is_some() {
            return Err(Failure::At(
                "WHERE clause not allowed with COPY TO".to_owned(),
                "42601",
                rhs.location(11),
            ));
        }
        for index in [2, 8] {
            if let Some(flag) = rhs.take(index).optional()? {
                statement.options.push(flag.into_inner());
            }
        }
        statement.options.extend(rhs.take(10).list()?);
    }
    Ok(Value::from_node(statement.into()))
}

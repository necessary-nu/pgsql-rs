//! XMLTABLE column options require validation before flattening into the raw node.
use super::{records::record, value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::*;

pub(super) fn option(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let name = rhs.take(1).text()?;
    if name == "__pg__is_not_null" {
        return Err(Failure::At(
            format!("option name \"{name}\" cannot be used in XMLTABLE"),
            "42601",
            rhs.location(1),
        ));
    }
    Ok(Value::from_node(
        DefElem {
            defname: Some(name),
            arg: Some(Box::new(rhs.take(2).node()?)),
            location: rhs.location(1),
            ..Default::default()
        }
        .into(),
    ))
}

pub(super) fn column(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let name = rhs.take(1).text()?;
    let mut column = RangeTableFuncCol {
        type_name: record!(rhs.take(2), TypeName),
        location: rhs.location(1),
        ..Default::default()
    };
    let mut nullability_seen = false;
    for option in rhs.take(3).list()? {
        let Node::DefElem(option) = option else {
            return Err("expected XMLTABLE column option".into());
        };
        let option = option.into_inner();
        let key = option
            .defname
            .as_deref()
            .ok_or("expected XMLTABLE option name")?;
        let error = match key {
            "default" if column.coldefexpr.is_some() => {
                Some("only one DEFAULT value is allowed".into())
            }
            "path" if column.colexpr.is_some() => {
                Some("only one PATH value per column is allowed".into())
            }
            "__pg__is_not_null" if nullability_seen => Some(format!(
                "conflicting or redundant NULL / NOT NULL declarations for column \"{name}\""
            )),
            "default" => {
                column.coldefexpr = option.arg;
                None
            }
            "path" => {
                column.colexpr = option.arg;
                None
            }
            "__pg__is_not_null" => {
                let Some(Node::Boolean(v)) = option.arg.as_deref() else {
                    return Err("expected XMLTABLE nullability".into());
                };
                column.is_not_null = *v;
                nullability_seen = true;
                None
            }
            _ => Some(format!("unrecognized column option \"{key}\"")),
        };
        if let Some(message) = error {
            return Err(Failure::At(message, "42601", option.location));
        }
    }
    column.colname = Some(name);
    Ok(Value::from_node(column.into()))
}

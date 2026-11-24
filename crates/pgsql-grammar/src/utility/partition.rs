use crate::raw::*;
use crate::{value::Value, Failure};
use pgsql_engine::Reduction;

pub(super) fn spec(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let name = rhs.take(3).text()?;
    let strategy = match name.to_ascii_lowercase().as_str() {
        "list" => PartitionStrategy::PARTITION_STRATEGY_LIST,
        "range" => PartitionStrategy::PARTITION_STRATEGY_RANGE,
        "hash" => PartitionStrategy::PARTITION_STRATEGY_HASH,
        _ => {
            return Err(Failure::At(
                format!(r#"unrecognized partitioning strategy "{name}""#),
                "22023",
                rhs.location(3),
            ))
        }
    };
    Ok(Value::from_node(
        PartitionSpec {
            strategy,
            location: rhs.location(1),
            part_params: rhs.take(5).list()?,
        }
        .into(),
    ))
}

pub(super) fn hash_bound(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let mut modulus = None;
    let mut remainder = None;
    for option in rhs.take(5).list()? {
        let Node::DefElem(option) = option else {
            return Err("expected hash bound option".into());
        };
        let name = option
            .defname
            .as_deref()
            .ok_or("expected bound option name")?;
        let dest = match name {
            "modulus" => &mut modulus,
            "remainder" => &mut remainder,
            _ => {
                return Err(Failure::At(
                    format!(r#"unrecognized hash partition bound specification "{name}""#),
                    "42601",
                    option.location,
                ))
            }
        };
        if dest.is_some() {
            return Err(Failure::At(
                format!("{name} for hash partition provided more than once"),
                "42710",
                option.location,
            ));
        }
        let Some(Node::Integer(n)) = option.arg.as_deref() else {
            return Err("expected integer hash bound".into());
        };
        *dest = Some(*n);
    }
    let location = rhs.location(3);
    let required = |v: Option<i32>, label: &str| {
        v.ok_or_else(|| {
            Failure::At(
                format!("{label} for hash partition must be specified"),
                "42601",
                location,
            )
        })
    };
    Ok(Value::from_node(
        PartitionBoundSpec {
            strategy: PartitionStrategy::PARTITION_STRATEGY_HASH.0 as u8,
            modulus: required(modulus, "modulus")?,
            remainder: required(remainder, "remainder")?,
            location,
            ..Default::default()
        }
        .into(),
    ))
}

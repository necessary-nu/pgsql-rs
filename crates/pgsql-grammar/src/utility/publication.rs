//! Publication lists carry the TABLE or TABLES IN SCHEMA qualifier to later entries.
use super::range_name;
use crate::raw::*;
use crate::{records::record, value::Value, Failure};
use pgsql_engine::Reduction;

type Kind = PublicationObjSpecType;

pub(super) fn continuation(rhs: &mut Reduction<'_, Value>, dotted: bool) -> Result<Value, Failure> {
    let offset = isize::from(dotted);
    let columns = rhs.take(2 + offset).list()?;
    let clause = rhs.take(3 + offset).optional()?;
    let mut spec = PublicationObjSpec {
        pubobjtype: Kind::PUBLICATIONOBJ_CONTINUATION,
        location: rhs.location(1),
        ..Default::default()
    };
    if !dotted && columns.is_empty() && clause.is_none() {
        spec.name = Some(rhs.take(1).text()?);
    } else {
        let relation = record!(super::qualified(rhs, dotted)?, RangeVar);
        spec.pubtable = Some(Box::new(PublicationTable {
            relation,
            columns,
            where_clause: clause,
            except: false,
        }));
    }
    Ok(Value::from_node(spec.into()))
}

fn normalize(objects: &mut [Node]) -> Result<(), Failure> {
    let mut previous = Kind::PUBLICATIONOBJ_CONTINUATION;
    for object in objects {
        let Node::PublicationObjSpec(spec) = object else {
            return Err("expected publication object".into());
        };
        if spec.pubobjtype == Kind::PUBLICATIONOBJ_CONTINUATION {
            if previous == Kind::PUBLICATIONOBJ_CONTINUATION {
                return Err(Failure::At(
                    "invalid publication object list".into(),
                    "42601",
                    spec.location,
                ));
            }
            spec.pubobjtype = previous;
        }
        let error = match spec.pubobjtype {
            Kind::PUBLICATIONOBJ_TABLE => {
                if let Some(name) = spec.name.take() {
                    spec.pubtable = Some(Box::new(PublicationTable {
                        relation: Some(Box::new(range_name([name], spec.location, true)?)),
                        ..Default::default()
                    }));
                }
                spec.pubtable.is_none().then_some("invalid table name")
            }
            Kind::PUBLICATIONOBJ_TABLES_IN_SCHEMA | Kind::PUBLICATIONOBJ_TABLES_IN_CUR_SCHEMA => {
                if let Some(table) = &spec.pubtable {
                    Some(if table.where_clause.is_some() {
                        "WHERE clause not allowed for schema"
                    } else if !table.columns.is_empty() {
                        "column specification not allowed for schema"
                    } else {
                        "invalid schema name"
                    })
                } else {
                    spec.pubobjtype = if spec.name.is_some() {
                        Kind::PUBLICATIONOBJ_TABLES_IN_SCHEMA
                    } else {
                        Kind::PUBLICATIONOBJ_TABLES_IN_CUR_SCHEMA
                    };
                    None
                }
            }
            _ => None,
        };
        if let Some(error) = error {
            return Err(Failure::At(error.to_owned(), "42601", spec.location));
        }
        previous = spec.pubobjtype;
    }
    Ok(())
}

fn all_objects(items: Vec<Node>) -> Result<(Vec<Node>, bool, bool), Failure> {
    let (mut tables, mut sequences, mut objects) = (false, false, vec![]);
    for item in items {
        let Node::PublicationAllObjSpec(mut spec) = item else {
            return Err("expected publication object type".into());
        };
        let flag = if spec.pubobjtype == PublicationAllObjType::PUBLICATION_ALL_TABLES {
            objects.append(&mut spec.except_tables);
            &mut tables
        } else {
            &mut sequences
        };
        if *flag {
            return Err(Failure::At(
                "invalid publication object list".into(),
                "42601",
                spec.location,
            ));
        }
        *flag = true;
    }
    Ok((objects, tables, sequences))
}

pub(super) fn publication(
    rhs: &mut Reduction<'_, Value>,
    action: Option<AlterPublicationAction>,
    all: bool,
) -> Result<Value, Failure> {
    let pubname = Some(rhs.take(3).text()?);
    let objects = rhs.take(5).list()?;
    let (pubobjects, for_all_tables, for_all_sequences) = if all {
        all_objects(objects)?
    } else {
        let mut objects = objects;
        normalize(&mut objects)?;
        (objects, false, false)
    };
    if let Some(action) = action {
        Ok(Value::from_node(
            AlterPublicationStmt {
                pubname,
                pubobjects,
                action,
                for_all_tables,
                for_all_sequences,
                ..Default::default()
            }
            .into(),
        ))
    } else {
        Ok(Value::from_node(
            CreatePublicationStmt {
                pubname,
                options: rhs.take(6).list()?,
                pubobjects,
                for_all_tables,
                for_all_sequences,
            }
            .into(),
        ))
    }
}

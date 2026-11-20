//! PostgreSQL utility grammar actions and their shared relation-name reductions.
pub(crate) mod access;
pub(crate) mod administration;
pub(crate) mod constraint;
mod copy;
pub(crate) mod definition;
mod foreign_key;
mod partition;
mod publication;
pub(crate) mod routine;
pub(crate) mod trigger;
pub(crate) mod view;

use super::{
    value::{OwnedValue, Value},
    Failure,
};
use pg19_engine::Reduction;
use pg19_nodes::*;

use crate::records::{optional_text, record};

pub(super) enum Utility {
    Cluster(isize, isize),
    VacuumLegacy,
    ExplainAnalyze,
    NegativeFloat,
    Signature(bool),
    CreateAs(view::Source, bool),
    RecursiveView(bool),
    GeneratedColumn,
    TableFunction,
    Definition(definition::Definition),
    PercentType(bool),
    RoutineBody,
    Transform,
    CreateSchema(bool),
    OfType,
    SetXmlOption,
    Unsupported(&'static str, isize),
    PartitionSpec,
    Publication(Option<AlterPublicationAction>, bool),
    PublicationTail(bool),
    HashBound,
    Reindex { relation: bool, schema: bool },
    AllColumns(isize),
    Copy(bool),
    Grant(bool, bool),
    Target(GrantTargetType, ObjectType, isize),
    RoleOption,
    NamedRole,
    RoleId,
    Qualified(bool),
    Relation(isize, bool),
}

fn flag(name: &str, location: Location) -> Node {
    DefElem {
        defname: Some(name.to_owned()),
        location,
        arg_location: Location::UNKNOWN,
        ..Default::default()
    }
    .into()
}

fn flag_options(
    rhs: &mut Reduction<'_, Value>,
    flags: &[(isize, &str)],
) -> Result<Vec<Node>, Failure> {
    let mut options = Vec::new();
    for &(index, name) in flags {
        if rhs.take(index).integer()? != 0 {
            options.push(flag(name, rhs.location(index)));
        }
    }
    Ok(options)
}

fn qualified(rhs: &mut Reduction<'_, Value>, dotted: bool) -> Result<Value, Failure> {
    let first = rhs.take(1).text()?;
    let location = rhs.location(1);
    if !dotted {
        return Ok(Value::from_node(
            range_name([first], location, true)?.into(),
        ));
    }
    let tail = match rhs.take(2) {
        Value::Owned(OwnedValue::Node(Node::String(field))) => {
            return Ok(Value::from_node(
                range_name([first, field], location, true)?.into(),
            ))
        }
        Value::Owned(OwnedValue::Node(_)) => return Err(Failure::Syntax("syntax error")),
        fields => fields.list()?,
    };
    let mut parts = Vec::with_capacity(1 + tail.len());
    parts.push(first);
    for value in tail {
        let Node::String(name) = value else {
            return Err(Failure::Syntax("syntax error"));
        };
        parts.push(name);
    }
    Ok(Value::from_node(range_name(parts, location, true)?.into()))
}

pub(super) fn range_name<I: IntoIterator<Item = String>>(
    parts: I,
    location: Location,
    inherit: bool,
) -> Result<RangeVar, Failure>
where
    I::IntoIter: DoubleEndedIterator + ExactSizeIterator,
{
    let mut parts = parts.into_iter();
    if !(1..=3).contains(&parts.len()) {
        return Err(Failure::At(
            format!(
                "improper qualified name (too many dotted names): {}",
                parts.collect::<Vec<_>>().join(".")
            ),
            "42601",
            location,
        ));
    }
    Ok(RangeVar {
        relname: parts.next_back(),
        schemaname: parts.next_back(),
        catalogname: parts.next_back(),
        location,
        inh: inherit,
        relpersistence: b'p',
        ..Default::default()
    })
}

// [spec:pgq:sem:grammar.utility-actions]
pub(super) fn reduce(op: Utility, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    match op {
        Utility::Unsupported(message, slot) => {
            Err(Failure::At(message.into(), "0A000", rhs.location(slot)))
        }
        Utility::SetXmlOption => {
            let xml = if rhs.take(3).integer()? == XmlOptionType::XMLOPTION_DOCUMENT.0 {
                "DOCUMENT"
            } else {
                "CONTENT"
            };
            Ok(Value::from_node(
                VariableSetStmt {
                    kind: VariableSetKind::VAR_SET_VALUE,
                    name: Some("xmloption".into()),
                    args: vec![Node::AConst(AConst {
                        val: Literal::String(xml.into()),
                        location: rhs.location(3),
                    })],
                    jumble_args: true,
                    location: Location::UNKNOWN,
                    ..Default::default()
                }
                .into(),
            ))
        }
        Utility::OfType => Ok(Value::from_node(
            AlterTableCmd {
                subtype: AlterTableType::AT_AddOf,
                def: Some(Box::new(
                    crate::types::named(rhs.take(2).list()?, Vec::new(), rhs.location(2)).into(),
                )),
                ..Default::default()
            }
            .into(),
        )),
        Utility::Transform => routine::transform(rhs),
        Utility::CreateSchema(authorized) => {
            let elements = if authorized { 9 } else { 7 };
            if !rhs.take(elements).list()?.is_empty() {
                return Err(Failure::At(
                    "CREATE SCHEMA IF NOT EXISTS cannot include schema elements".into(),
                    "0A000",
                    rhs.location(elements),
                ));
            }
            let authrole = if authorized {
                record!(rhs.take(8), RoleSpec)
            } else {
                None
            };
            Ok(Value::from_node(
                CreateSchemaStmt {
                    schemaname: optional_text(rhs.take(6))?,
                    authrole,
                    if_not_exists: true,
                    ..Default::default()
                }
                .into(),
            ))
        }
        Utility::Cluster(rel, idx) => {
            let relation = record!(rhs.take(rel), RangeVar);
            let params = flag_options(rhs, &[(2, "verbose")])?;
            Ok(Value::from_node(
                RepackStmt {
                    command: RepackCommand::REPACK_COMMAND_CLUSTER,
                    relation: Some(Box::new(VacuumRelation {
                        relation,
                        ..Default::default()
                    })),
                    indexname: optional_text(rhs.take(idx))?,
                    usingindex: true,
                    params,
                }
                .into(),
            ))
        }
        Utility::VacuumLegacy => {
            let options = flag_options(
                rhs,
                &[(2, "full"), (3, "freeze"), (4, "verbose"), (5, "analyze")],
            )?;
            Ok(Value::from_node(
                VacuumStmt {
                    options,
                    rels: rhs.take(6).list()?,
                    is_vacuumcmd: true,
                }
                .into(),
            ))
        }
        Utility::ExplainAnalyze => {
            let mut options = vec![flag("analyze", rhs.location(2))];
            options.extend(flag_options(rhs, &[(3, "verbose")])?);
            Ok(Value::from_node(
                ExplainStmt {
                    options,
                    query: rhs.take(4).optional()?,
                }
                .into(),
            ))
        }
        Utility::NamedRole => {
            let name = rhs.take(1).text()?;
            let location = rhs.location(1);
            let (roletype, rolename) = match name.as_str() {
                "none" => {
                    return Err(Failure::At(
                        "role name \"none\" is reserved".to_owned(),
                        "42939",
                        location,
                    ))
                }
                "public" => (RoleSpecType::ROLESPEC_PUBLIC, None),
                _ => (RoleSpecType::ROLESPEC_CSTRING, Some(name)),
            };
            Ok(Value::from_node(
                RoleSpec {
                    roletype,
                    rolename,
                    location,
                }
                .into(),
            ))
        }
        Utility::RoleId => {
            let Node::RoleSpec(mut role) = rhs.take(1).node()? else {
                return Err("expected RoleSpec".into());
            };
            if role.roletype == RoleSpecType::ROLESPEC_CSTRING {
                return Ok(Value::from_text(
                    role.rolename.take().ok_or("expected role name")?,
                ));
            }
            let message = match role.roletype {
                RoleSpecType::ROLESPEC_PUBLIC => "role name \"public\" is reserved".to_owned(),
                role => {
                    let name = match role {
                        RoleSpecType::ROLESPEC_CURRENT_ROLE => "CURRENT_ROLE",
                        RoleSpecType::ROLESPEC_CURRENT_USER => "CURRENT_USER",
                        RoleSpecType::ROLESPEC_SESSION_USER => "SESSION_USER",
                        _ => return Err("unknown role specification".into()),
                    };
                    format!("{name} cannot be used as a role name here")
                }
            };
            Err(Failure::At(message, "42939", rhs.location(1)))
        }
        Utility::Target(kind, object_type, index) => Ok(Value::Owned(OwnedValue::PrivilegeTarget(
            access::PrivilegeTarget {
                kind,
                object_type,
                objects: rhs.take(index).list()?,
            },
        ))),
        Utility::Grant(grant, for_option) => access::grant(rhs, grant, for_option),
        Utility::RoleOption => access::role_option(rhs),
        Utility::Reindex { relation, schema } => {
            let kind = if schema {
                ReindexObjectType::REINDEX_OBJECT_SCHEMA
            } else {
                ReindexObjectType(rhs.take(3).integer()?)
            };
            let (relation, name) = if relation {
                (record!(rhs.take(5), RangeVar), None)
            } else {
                (None, optional_text(rhs.take(5))?)
            };
            let mut params = rhs.take(2).list()?;
            params.extend(flag_options(rhs, &[(4, "concurrently")])?);
            Ok(Value::from_node(
                ReindexStmt {
                    kind,
                    relation,
                    name,
                    params,
                }
                .into(),
            ))
        }
        Utility::AllColumns(index) => Ok(Value::from_list(vec![AccessPriv {
            priv_name: None,
            cols: rhs.take(index).list()?,
        }
        .into()])),
        Utility::Definition(action) => action.reduce(rhs),
        Utility::CreateAs(source, exists) => view::create_as(rhs, source, exists),
        Utility::RecursiveView(replace) => view::recursive(rhs, replace),
        Utility::GeneratedColumn => constraint::generated_column(rhs),
        Utility::Signature(qualified) => routine::signature(rhs, qualified),
        Utility::TableFunction => routine::table_function(rhs),
        Utility::PercentType(setof) => routine::percent_type(rhs, setof),
        Utility::RoutineBody => routine::body(rhs),
        Utility::PartitionSpec => partition::spec(rhs),
        Utility::Publication(action, all) => publication::publication(rhs, action, all),
        Utility::PublicationTail(dotted) => publication::continuation(rhs, dotted),
        Utility::HashBound => partition::hash_bound(rhs),

        Utility::Copy(query) => copy::copy(rhs, query),
        Utility::NegativeFloat => {
            let text = rhs.take(2).text()?;
            let digits = text.strip_prefix('+').unwrap_or(&text);
            let number = match digits.strip_prefix('-') {
                Some(positive) => positive.to_owned(),
                None => format!("-{digits}"),
            };
            Ok(Value::from_node(Node::Float(number)))
        }
        Utility::Qualified(dotted) => qualified(rhs, dotted),
        Utility::Relation(index, inherit) => {
            let Node::RangeVar(mut range) = rhs.take(index).node()? else {
                return Err("expected relation name".into());
            };
            range.inh = inherit;
            range.alias = None;
            Ok(Value::from_node(Node::RangeVar(range)))
        }
    }
}

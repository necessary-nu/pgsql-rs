//! SQL/JSON raw grammar actions. PostgreSQL constructs these trees before
//! checking path syntax, coercing values or resolving catalog types.
use super::{
    records::{optional_text, record},
    value::Value,
    Failure,
};
use crate::raw::*;
use pgsql_engine::Reduction;

pub(super) enum Json {
    Format { explicit: bool, encoding: bool },
    Value,
    Returning,
    KeyValue,
    Is(bool),
    Behavior(bool),
    Function(JsonExprOp),
    Aggregate(bool),
    AggregateOptions,
    Path,
    Table,
    Column(JsonTableColumnType),
    Nested(bool),
}

fn default_format() -> JsonFormat {
    JsonFormat {
        format_type: JsonFormatType::JS_FORMAT_DEFAULT,
        encoding: JsonEncoding::JS_ENC_DEFAULT,
        location: Location::UNKNOWN,
    }
}

type Behaviors = (Option<Box<JsonBehavior>>, Option<Box<JsonBehavior>>);
fn behaviors(value: Value) -> Result<Behaviors, Failure> {
    let [empty, error]: [Node; 2] = value
        .list()?
        .try_into()
        .map_err(|_| "expected two JSON behaviors")?;
    fn unpack(n: Node) -> Result<Option<Box<JsonBehavior>>, Failure> {
        match n {
            Node::Null => Ok(None),
            Node::JsonBehavior(b) => Ok(Some(b)),
            _ => Err("expected JSON behavior".into()),
        }
    }
    Ok((unpack(empty)?, unpack(error)?))
}

fn path(
    string: String,
    location: Location,
    name: Option<String>,
    name_location: Location,
) -> Box<JsonTablePathSpec> {
    Box::new(JsonTablePathSpec {
        string: Some(Box::new(Node::AConst(AConst {
            val: Literal::String(string),
            location,
        }))),
        name,
        name_location,
        location,
    })
}

fn json_function(op: JsonExprOp, rhs: &mut Reduction<'_, Value>) -> Result<Node, Failure> {
    let mut f = JsonFuncExpr {
        op,
        context_item: record!(rhs.take(3), JsonValueExpr),
        pathspec: Some(Box::new(rhs.take(5).node()?)),
        passing: rhs.take(6).list()?,
        location: rhs.location(1),
        ..Default::default()
    };
    if op == JsonExprOp::JSON_EXISTS_OP {
        f.on_error = record!(rhs.take(7), JsonBehavior);
    } else {
        f.output = record!(rhs.take(7), JsonOutput);
        let b = if op == JsonExprOp::JSON_QUERY_OP {
            f.wrapper = JsonWrapper(rhs.take(8).integer()?);
            f.quotes = JsonQuotes(rhs.take(9).integer()?);
            10
        } else {
            8
        };
        (f.on_empty, f.on_error) = behaviors(rhs.take(b))?;
    }
    Ok(f.into())
}

fn column(ty: JsonTableColumnType, rhs: &mut Reduction<'_, Value>) -> Result<Node, Failure> {
    let formatted = ty == JsonTableColumnType::JTC_FORMATTED;
    let exists = ty == JsonTableColumnType::JTC_EXISTS;
    let p = if formatted || exists { 4 } else { 3 };
    let mut column = JsonTableColumn {
        coltype: ty,
        name: Some(rhs.take(1).text()?),
        type_name: record!(rhs.take(2), TypeName),
        pathspec: record!(rhs.take(p), JsonTablePathSpec),
        format: if formatted {
            record!(rhs.take(3), JsonFormat)
        } else {
            Some(Box::new(default_format()))
        },
        location: rhs.location(1),
        ..Default::default()
    };
    if exists {
        column.wrapper = JsonWrapper::JSW_NONE;
        column.on_error = record!(rhs.take(p + 1), JsonBehavior);
    } else {
        column.wrapper = JsonWrapper(rhs.take(p + 1).integer()?);
        column.quotes = JsonQuotes(rhs.take(p + 2).integer()?);
        (column.on_empty, column.on_error) = behaviors(rhs.take(p + 3))?;
    }
    Ok(column.into())
}

pub(super) fn reduce(op: Json, rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let node = match op {
        Json::Format { explicit, encoding } => {
            let mut f = default_format();
            if explicit {
                f.format_type = JsonFormatType::JS_FORMAT_JSON;
                f.location = rhs.location(1);
            }
            if encoding {
                let name = rhs.take(4).text()?;
                f.encoding = match name.owned().to_ascii_lowercase().as_str() {
                    "utf8" => JsonEncoding::JS_ENC_UTF8,
                    "utf16" => JsonEncoding::JS_ENC_UTF16,
                    "utf32" => JsonEncoding::JS_ENC_UTF32,
                    _ => {
                        return Err(Failure::At(
                            format!("unrecognized JSON encoding: {name}"),
                            "22023",
                            rhs.location(4),
                        ))
                    }
                };
            }
            f.into()
        }
        Json::Value => JsonValueExpr {
            raw_expr: Some(Box::new(rhs.take(1).node()?)),
            format: record!(rhs.take(2), JsonFormat),
            formatted_expr: None,
        }
        .into(),
        Json::Returning => JsonOutput {
            type_name: record!(rhs.take(2), TypeName),
            returning: Some(Box::new(JsonReturning {
                format: record!(rhs.take(3), JsonFormat),
                ..Default::default()
            })),
        }
        .into(),
        Json::KeyValue => JsonKeyValue {
            key: Some(Box::new(rhs.take(1).node()?)),
            value: record!(rhs.take(3), JsonValueExpr),
        }
        .into(),
        Json::Is(negated) => {
            let i = if negated { 4 } else { 3 };
            let location = rhs.location(1);
            let predicate = JsonIsPredicate {
                expr: Some(Box::new(rhs.take(1).node()?)),
                format: Some(Box::new(default_format())),
                item_type: JsonValueType(rhs.take(i).integer()?),
                unique_keys: rhs.take(i + 1).integer()? != 0,
                location,
                ..Default::default()
            }
            .into();
            if negated {
                BoolExpr {
                    boolop: BoolExprType::NOT_EXPR,
                    args: vec![predicate],
                    location,
                }
                .into()
            } else {
                predicate
            }
        }
        Json::Behavior(default) => JsonBehavior {
            btype: if default {
                JsonBehaviorType::JSON_BEHAVIOR_DEFAULT
            } else {
                JsonBehaviorType(rhs.take(1).integer()?)
            },
            expr: if default {
                Some(Box::new(rhs.take(2).node()?))
            } else {
                None
            },
            location: rhs.location(1),
            ..Default::default()
        }
        .into(),
        Json::Function(op) => json_function(op, rhs)?,
        Json::Aggregate(object) => {
            let constructor = Some(Box::new(JsonAggConstructor {
                output: record!(rhs.take(6), JsonOutput),
                agg_order: if object {
                    Vec::new()
                } else {
                    rhs.take(4).list()?
                },
                location: rhs.location(1),
                ..Default::default()
            }));
            if object {
                JsonObjectAgg {
                    constructor,
                    arg: record!(rhs.take(3), JsonKeyValue),
                    absent_on_null: rhs.take(4).integer()? != 0,
                    unique: rhs.take(5).integer()? != 0,
                }
                .into()
            } else {
                JsonArrayAgg {
                    constructor,
                    arg: record!(rhs.take(3), JsonValueExpr),
                    absent_on_null: rhs.take(5).integer()? != 0,
                }
                .into()
            }
        }
        Json::AggregateOptions => {
            let mut agg = rhs.take(1).node()?;
            let constructor = match &mut agg {
                Node::JsonObjectAgg(a) => a.load().constructor,
                Node::JsonArrayAgg(a) => a.load().constructor,
                _ => None,
            }
            .ok_or("expected JSON aggregate constructor")?;
            constructor.set_field(|node| &mut node.agg_filter, rhs.take(2).optional()?);
            constructor.set_field(|node| &mut node.over, record!(rhs.take(3), WindowDef));
            agg
        }
        Json::Path => path(
            rhs.take(2).text()?,
            rhs.location(2),
            None,
            Location::UNKNOWN,
        )
        .into(),
        Json::Table => {
            let Node::AConst(AConst {
                val: Literal::String(string),
                ..
            }) = rhs.take(5).node()?
            else {
                return Err(Failure::At(
                    "only string constants are supported in JSON_TABLE path specification".into(),
                    "0A000",
                    rhs.location(5),
                ));
            };
            JsonTable {
                context_item: record!(rhs.take(3), JsonValueExpr),
                pathspec: Some(path(
                    string,
                    rhs.location(5),
                    optional_text(rhs.take(6))?,
                    rhs.location(6),
                )),
                passing: rhs.take(7).list()?,
                columns: rhs.take(10).list()?,
                on_error: record!(rhs.take(12), JsonBehavior),
                location: rhs.location(1),
                ..Default::default()
            }
            .into()
        }
        Json::Column(ty) => column(ty, rhs)?,
        Json::Nested(named) => {
            let (name, name_location) = if named {
                (Some(rhs.take(5).text()?), rhs.location(5))
            } else {
                (None, Location::UNKNOWN)
            };
            JsonTableColumn {
                coltype: JsonTableColumnType::JTC_NESTED,
                pathspec: Some(path(
                    rhs.take(3).text()?,
                    rhs.location(3),
                    name,
                    name_location,
                )),
                columns: rhs.take(if named { 8 } else { 6 }).list()?,
                location: rhs.location(1),
                ..Default::default()
            }
            .into()
        }
    };
    Ok(Value::from_node(node))
}

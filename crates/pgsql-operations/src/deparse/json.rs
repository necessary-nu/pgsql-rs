// SQL/JSON constructors, accessors and JSON_TABLE follow the raw PG19 grammar.
use super::operator::{operand, Prec};
use super::query::{alias, named};
use super::writer::{required, Plan, Role};
use super::{Error, Result};
use pgsql_nodes::*;

fn format(f: Option<&JsonFormat>, p: &mut Plan<'_>) -> Result<()> {
    if let Some(f) = f {
        match f.format_type {
            JsonFormatType::JS_FORMAT_DEFAULT => return Ok(()),
            JsonFormatType::JS_FORMAT_JSON => p.text(" FORMAT JSON"),
            JsonFormatType::JS_FORMAT_JSONB => p.text(" FORMAT JSONB"),
            _ => return Err(Error::InvalidField("JsonFormat.format_type")),
        }
        p.text(match f.encoding {
            JsonEncoding::JS_ENC_DEFAULT => "",
            JsonEncoding::JS_ENC_UTF8 => " ENCODING utf8",
            JsonEncoding::JS_ENC_UTF16 => " ENCODING utf16",
            JsonEncoding::JS_ENC_UTF32 => " ENCODING utf32",
            _ => return Err(Error::InvalidField("JsonFormat.encoding")),
        });
    }
    Ok(())
}
fn value<'a>(v: &'a JsonValueExpr, p: &mut Plan<'a>) -> Result<()> {
    p.expr(required(&v.raw_expr, "JsonValueExpr.raw_expr")?);
    format(v.format.as_deref(), p)
}
fn keyvalue<'a>(kv: &'a JsonKeyValue, p: &mut Plan<'a>) -> Result<()> {
    p.expr(required(&kv.key, "JsonKeyValue.key")?);
    p.text(": ");
    value(required(&kv.value, "JsonKeyValue.value")?, p)
}
fn output<'a>(out: Option<&'a JsonOutput>, p: &mut Plan<'a>) -> Result<()> {
    if let Some(out) = out {
        p.text(" RETURNING ");
        p.typename(required(&out.type_name, "JsonOutput.typeName")?);
        format(
            required(&out.returning, "JsonOutput.returning")?
                .format
                .as_deref(),
            p,
        )?;
    }
    Ok(())
}
fn wrapper(w: JsonWrapper, p: &mut Plan<'_>) -> Result<()> {
    p.text(match w {
        JsonWrapper::JSW_UNSPEC => "",
        JsonWrapper::JSW_NONE => " WITHOUT WRAPPER",
        JsonWrapper::JSW_CONDITIONAL => " WITH CONDITIONAL WRAPPER",
        JsonWrapper::JSW_UNCONDITIONAL => " WITH UNCONDITIONAL WRAPPER",
        _ => return Err(Error::InvalidField("JSON wrapper")),
    });
    Ok(())
}
fn quotes(q: JsonQuotes, p: &mut Plan<'_>) -> Result<()> {
    p.text(match q {
        JsonQuotes::JS_QUOTES_UNSPEC => "",
        JsonQuotes::JS_QUOTES_KEEP => " KEEP QUOTES",
        JsonQuotes::JS_QUOTES_OMIT => " OMIT QUOTES",
        _ => return Err(Error::InvalidField("JSON quotes")),
    });
    Ok(())
}
fn behavior<'a>(b: Option<&'a JsonBehavior>, suffix: &'static str, p: &mut Plan<'a>) -> Result<()> {
    if let Some(b) = b {
        if b.btype == JsonBehaviorType::JSON_BEHAVIOR_DEFAULT {
            p.text(" DEFAULT ");
            p.expr(required(&b.expr, "JsonBehavior.expr")?);
        } else {
            p.text(" ");
            p.text(
                b.btype
                    .pg_name()
                    .and_then(|n| n.strip_prefix("JSON_BEHAVIOR_"))
                    .ok_or(Error::InvalidField("JsonBehavior.btype"))?
                    .replace('_', " "),
            );
        }
        p.text(suffix);
    }
    Ok(())
}
fn passing<'a>(args: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    if !args.is_empty() {
        p.text(" PASSING ");
    }
    for (i, n) in args.iter().enumerate() {
        let Node::JsonArgument(a) = n else {
            return Err(Error::InvalidField("JSON passing arguments"));
        };
        if i > 0 {
            p.text(", ");
        }
        value(required(&a.val, "JsonArgument.val")?, p)?;
        p.text("  AS ");
        p.text(named(&a.name, "JsonArgument.name")?);
    }
    Ok(())
}
fn function<'a>(f: &'a JsonFuncExpr, p: &mut Plan<'a>) -> Result<()> {
    p.text(match f.op {
        JsonExprOp::JSON_EXISTS_OP => "JSON_EXISTS(",
        JsonExprOp::JSON_QUERY_OP => "JSON_QUERY(",
        JsonExprOp::JSON_VALUE_OP => "JSON_VALUE(",
        _ => return Err(Error::InvalidField("JsonFuncExpr.op")),
    });
    value(required(&f.context_item, "JsonFuncExpr.context_item")?, p)?;
    p.text(" , ");
    p.expr(required(&f.pathspec, "JsonFuncExpr.pathspec")?);
    passing(&f.passing, p)?;
    output(f.output.as_deref(), p)?;
    if f.output.is_some() {
        p.text(" ");
    }
    wrapper(f.wrapper, p)?;
    quotes(f.quotes, p)?;
    behavior(f.on_empty.as_deref(), " ON EMPTY", p)?;
    behavior(f.on_error.as_deref(), " ON ERROR", p)?;
    p.text(")");
    Ok(())
}
fn obj_options(absent: bool, unique: bool, p: &mut Plan<'_>) {
    if absent {
        p.text(" ABSENT ON NULL");
    }
    if unique {
        p.text(" WITH UNIQUE");
    }
}
fn agg_tail<'a>(ctor: &'a JsonAggConstructor, p: &mut Plan<'a>) -> Result<()> {
    output(ctor.output.as_deref(), p)?;
    p.text(")");
    if let Some(filter) = &ctor.agg_filter {
        p.text(" FILTER (WHERE ");
        p.expr(filter);
        p.text(")");
    }
    if let Some(w) = &ctor.over {
        super::window::over(w, p)?;
    }
    Ok(())
}
fn constructors<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<()> {
    match n {
        Node::JsonObjectConstructor(o) => {
            p.text("JSON_OBJECT(");
            for (j, kv) in o.exprs.iter().enumerate() {
                let Node::JsonKeyValue(kv) = kv else {
                    return Err(Error::InvalidField("JsonObjectConstructor.exprs"));
                };
                if j > 0 {
                    p.text(", ");
                }
                keyvalue(kv, p)?;
            }
            obj_options(o.absent_on_null, o.unique, p);
            output(o.output.as_deref(), p)?;
        }
        Node::JsonArrayConstructor(a) => {
            p.text("JSON_ARRAY(");
            for (j, v) in a.exprs.iter().enumerate() {
                let Node::JsonValueExpr(v) = v else {
                    return Err(Error::InvalidField("JsonArrayConstructor.exprs"));
                };
                if j > 0 {
                    p.text(", ");
                }
                value(v, p)?;
            }
            if !a.absent_on_null {
                p.text(" NULL ON NULL");
            }
            output(a.output.as_deref(), p)?;
        }
        Node::JsonArrayQueryConstructor(a) => {
            p.text("JSON_ARRAY(");
            p.statement(required(&a.query, "JsonArrayQueryConstructor.query")?);
            format(a.format.as_deref(), p)?;
            output(a.output.as_deref(), p)?;
        }
        Node::JsonObjectAgg(a) => {
            p.text("JSON_OBJECTAGG(");
            keyvalue(required(&a.arg, "JsonObjectAgg.arg")?, p)?;
            obj_options(a.absent_on_null, a.unique, p);
            return agg_tail(required(&a.constructor, "JsonObjectAgg.constructor")?, p);
        }
        Node::JsonArrayAgg(a) => {
            p.text("JSON_ARRAYAGG(");
            value(required(&a.arg, "JsonArrayAgg.arg")?, p)?;
            let ctor = required(&a.constructor, "JsonArrayAgg.constructor")?;
            if !ctor.agg_order.is_empty() {
                p.text(" ORDER BY ");
                for (j, v) in ctor.agg_order.iter().enumerate() {
                    if j > 0 {
                        p.text(", ");
                    }
                    p.node(v, Role::Sort);
                }
            }
            if !a.absent_on_null {
                p.text(" NULL ON NULL");
            }
            return agg_tail(ctor, p);
        }
        _ => return Err(Error::UnsupportedStatement(n.tag())),
    }
    p.text(")");
    Ok(())
}
pub(super) fn expression<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<()> {
    match n {
        Node::JsonIsPredicate(j) => {
            operand(
                p,
                required(&j.expr, "JsonIsPredicate.expr")?,
                Prec::Is,
                false,
            );
            format(j.format.as_deref(), p)?;
            p.text(match j.item_type {
                JsonValueType::JS_TYPE_ANY => " IS JSON",
                JsonValueType::JS_TYPE_ARRAY => " IS JSON ARRAY",
                JsonValueType::JS_TYPE_OBJECT => " IS JSON OBJECT",
                JsonValueType::JS_TYPE_SCALAR => " IS JSON SCALAR",
                _ => return Err(Error::InvalidField("JsonIsPredicate.item_type")),
            });
            if j.unique_keys {
                p.text(" WITH UNIQUE");
            }
        }
        Node::JsonParseExpr(j) => {
            p.text("JSON(");
            value(required(&j.expr, "JsonParseExpr.expr")?, p)?;
            p.text(" ");
            if j.unique_keys {
                p.text(" WITH UNIQUE KEYS");
            }
            output(j.output.as_deref(), p)?;
            p.text(")");
        }
        Node::JsonScalarExpr(j) => {
            p.text("JSON_SCALAR(");
            p.expr(required(&j.expr, "JsonScalarExpr.expr")?);
            output(j.output.as_deref(), p)?;
            p.text(")");
        }
        Node::JsonSerializeExpr(j) => {
            p.text("JSON_SERIALIZE(");
            value(required(&j.expr, "JsonSerializeExpr.expr")?, p)?;
            output(j.output.as_deref(), p)?;
            p.text(" )");
        }
        Node::JsonFuncExpr(f) => return function(f, p),
        _ => return constructors(n, p),
    }
    Ok(())
}
fn path<'a>(s: &'a JsonTablePathSpec, p: &mut Plan<'a>) -> Result<()> {
    p.expr(required(&s.string, "JsonTablePathSpec.string")?);
    if s.name.is_some() {
        p.text(" AS ");
        p.text(named(&s.name, "JsonTablePathSpec.name")?);
    }
    Ok(())
}
fn columns<'a>(cols: &'a [Node], p: &mut Plan<'a>) {
    p.text(" COLUMNS (");
    p.list(cols, Role::JsonColumn);
    p.text(")");
}
pub(super) fn column<'a>(c: &'a JsonTableColumn, p: &mut Plan<'a>) -> Result<()> {
    if c.coltype == JsonTableColumnType::JTC_NESTED {
        p.text("NESTED PATH ");
        path(required(&c.pathspec, "JsonTableColumn.pathspec")?, p)?;
        columns(&c.columns, p);
        return Ok(());
    }
    p.text(named(&c.name, "JsonTableColumn.name")?);
    p.text(" ");
    if c.coltype == JsonTableColumnType::JTC_FOR_ORDINALITY {
        p.text(" FOR ORDINALITY");
        return Ok(());
    }
    p.typename(required(&c.type_name, "JsonTableColumn.typeName")?);
    if c.coltype == JsonTableColumnType::JTC_EXISTS {
        p.text(" EXISTS");
    }
    format(c.format.as_deref(), p)?;
    p.text(" ");
    if let Some(s) = &c.pathspec {
        p.text("PATH ");
        path(s, p)?;
    }
    if c.coltype != JsonTableColumnType::JTC_EXISTS || c.wrapper != JsonWrapper::JSW_NONE {
        wrapper(c.wrapper, p)?;
    }
    quotes(c.quotes, p)?;
    behavior(c.on_empty.as_deref(), " ON EMPTY", p)?;
    behavior(c.on_error.as_deref(), " ON ERROR", p)?;
    Ok(())
}
pub(super) fn table<'a>(j: &'a JsonTable, p: &mut Plan<'a>) -> Result<()> {
    if j.lateral {
        p.text("LATERAL ");
    }
    p.text("JSON_TABLE(");
    value(required(&j.context_item, "JsonTable.context_item")?, p)?;
    p.text(" , ");
    path(required(&j.pathspec, "JsonTable.pathspec")?, p)?;
    passing(&j.passing, p)?;
    columns(&j.columns, p);
    behavior(j.on_error.as_deref(), " ON ERROR", p)?;
    p.text(")");
    if let Some(a) = &j.alias {
        p.text(" ");
        p.text(alias(a)?);
    }
    Ok(())
}

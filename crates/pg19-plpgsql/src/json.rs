use crate::{Block, DataType, Datum, Expression, ParseMode, Statement};
use serde_json::{json, Value};

fn expression(e: &Expression) -> Value {
    let m = match e.mode {
        ParseMode::Sql => 0,
        ParseMode::TypeName => 1,
        ParseMode::PlPgSqlExpr => 2,
        ParseMode::PlPgSqlAssign1 => 3,
        ParseMode::PlPgSqlAssign2 => 4,
        ParseMode::PlPgSqlAssign3 => 5,
    };
    json!({"PLpgSQL_expr":{"query":e.query,"parseMode":m}})
}

pub fn datum(d: &Datum, id: usize) -> Value {
    let mut v = json!({});
    match d {
        Datum::Variable(var) => {
            v["refname"] = json!(var.name);
            if var.lineno > 0 {
                v["lineno"] = json!(var.lineno);
            }
            if var.constant {
                v["isconst"] = json!(true);
            }
            if var.not_null {
                v["notnull"] = json!(true);
            }
            if let Some(default) = &var.default {
                v["default_val"] = expression(default);
            }
            match &var.datatype {
                DataType::Scalar(name) => {
                    v["datatype"] = json!({"PLpgSQL_type":{"typname":name}});
                    json!({"PLpgSQL_var":v})
                }
                DataType::Pseudo(_) => unreachable!("pseudo-variable in compiled tree"),
                DataType::Record => {
                    if id > 0 {
                        v["dno"] = json!(id);
                    }
                    json!({"PLpgSQL_rec":v})
                }
            }
        }
        Datum::Promise { variable, promise } => {
            let mut v = datum(&Datum::Variable(variable.clone()), id);
            v["PLpgSQL_var"]["promise"] = json!(promise);
            v
        }
        Datum::Row { fields } => {
            let values: Vec<_> = fields
                .iter()
                .map(|(name, id)| {
                    let mut v = json!({"name":name});
                    if *id > 0 {
                        v["varno"] = json!(id);
                    }
                    v
                })
                .collect();
            json!({"PLpgSQL_row":{"refname":"(unnamed row)", "lineno":-1, "fields":values}})
        }
        Datum::RecordField { name, parent } => {
            v["fieldname"] = json!(name);
            if *parent > 0 {
                v["recparentno"] = json!(parent);
            }
            json!({"PLpgSQL_recfield":v})
        }
    }
}

pub fn block(b: &Block) -> Value {
    let mut v = json!({});
    if b.lineno > 0 {
        v["lineno"] = json!(b.lineno);
    }
    if let Some(label) = &b.label {
        v["label"] = json!(label);
    }
    if !b.body.is_empty() {
        v["body"] = b
            .body
            .iter()
            .map(|s| match s {
                Statement::Return {
                    lineno,
                    expression: expr,
                    ..
                } => {
                    let mut v = json!({});
                    if *lineno > 0 {
                        v["lineno"] = json!(lineno);
                    }
                    if let Some(e) = expr {
                        v["expr"] = expression(e);
                    }
                    json!({"PLpgSQL_stmt_return":v})
                }
                Statement::Block(inner) => block(inner),
                Statement::Assign {
                    lineno,
                    datum,
                    expression: e,
                } => {
                    let mut v = json!({"lineno":lineno, "expr":expression(e)});
                    if *datum > 0 {
                        v["varno"] = json!(datum);
                    }
                    json!({"PLpgSQL_stmt_assign":v})
                }
                Statement::Perform {
                    lineno,
                    expression: e,
                } => json!({"PLpgSQL_stmt_perform":{"lineno":lineno,"expr":expression(e)}}),
            })
            .collect();
    }
    json!({"PLpgSQL_stmt_block":v})
}

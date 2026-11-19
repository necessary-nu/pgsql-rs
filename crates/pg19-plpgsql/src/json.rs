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
        Datum::Row { lineno, fields } => {
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
            json!({"PLpgSQL_row":{"refname":"(unnamed row)", "lineno":lineno, "fields":values}})
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
    statements(&mut v, "body", &b.body);
    json!({"PLpgSQL_stmt_block":v})
}

fn statements(v: &mut Value, key: &str, body: &[Statement]) {
    if !body.is_empty() {
        v[key] = body.iter().map(statement).collect();
    }
}

fn optional_expr(v: &mut Value, name: &str, e: &Option<Expression>) {
    if let Some(e) = e {
        v[name] = expression(e);
    }
}

fn statement(s: &Statement) -> Value {
    match s {
        Statement::If {
            lineno,
            condition,
            then_body,
            elsif,
            else_body,
        } => {
            let mut v = json!({"lineno":lineno,"cond":expression(condition)});
            statements(&mut v, "then_body", then_body);
            if !elsif.is_empty() {
                v["elsif_list"] = elsif
                    .iter()
                    .map(|el| {
                        let mut branch =
                            json!({"lineno":el.lineno,"cond":expression(&el.condition)});
                        statements(&mut branch, "stmts", &el.body);
                        json!({"PLpgSQL_if_elsif":branch})
                    })
                    .collect();
            }
            statements(&mut v, "else_body", else_body);
            json!({"PLpgSQL_stmt_if":v})
        }
        Statement::Loop {
            lineno,
            label,
            condition,
            body,
        } => {
            let mut v = json!({"lineno":lineno});
            if let Some(label) = label {
                v["label"] = json!(label);
            }
            optional_expr(&mut v, "cond", condition);
            statements(&mut v, "body", body);
            if condition.is_some() {
                json!({"PLpgSQL_stmt_while":v})
            } else {
                json!({"PLpgSQL_stmt_loop":v})
            }
        }
        Statement::Exit {
            lineno,
            is_exit,
            label,
            condition,
        } => {
            let mut v = json!({"lineno":lineno});
            if *is_exit {
                v["is_exit"] = json!(true);
            }
            if let Some(label) = label {
                v["label"] = json!(label);
            }
            optional_expr(&mut v, "cond", condition);
            json!({"PLpgSQL_stmt_exit":v})
        }
        Statement::Assert {
            lineno,
            condition,
            message,
        } => {
            let mut v = json!({"lineno":lineno,"cond":expression(condition)});
            optional_expr(&mut v, "message", message);
            json!({"PLpgSQL_stmt_assert":v})
        }
        Statement::Call {
            lineno,
            is_call,
            expression: e,
        } => {
            let mut v = json!({"lineno":lineno,"expr":expression(e)});
            if *is_call {
                v["is_call"] = json!(true);
            }
            json!({"PLpgSQL_stmt_call":v})
        }
        Statement::Transaction {
            lineno,
            commit,
            chain,
        } => {
            let mut v = json!({"lineno":lineno});
            if *chain {
                v["chain"] = json!(true);
            }
            if *commit {
                json!({"PLpgSQL_stmt_commit":v})
            } else {
                json!({"PLpgSQL_stmt_rollback":v})
            }
        }
        Statement::ExecSql {
            lineno,
            expression: e,
            target,
        } => {
            let mut v = json!({"lineno":lineno,"sqlstmt":expression(e)});
            if let Some(t) = target {
                v["into"] = json!(true);
                if t.strict {
                    v["strict"] = json!(true);
                }
                v["target"] = datum(&t.variable, t.id);
            }
            json!({"PLpgSQL_stmt_execsql":v})
        }
        Statement::ReturnNext {
            lineno,
            expression: e,
            ..
        } => {
            let mut v = json!({"lineno":lineno});
            optional_expr(&mut v, "expr", e);
            json!({"PLpgSQL_stmt_return_next":v})
        }
        Statement::ReturnQuery {
            lineno,
            query,
            dynamic,
            params,
        } => {
            let mut v = json!({"lineno":lineno});
            v[if *dynamic { "dynquery" } else { "query" }] = expression(query);
            if !params.is_empty() {
                v["params"] = params.iter().map(expression).collect();
            }
            json!({"PLpgSQL_stmt_return_query":v})
        }
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
    }
}

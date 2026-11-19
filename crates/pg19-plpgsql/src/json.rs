use crate::{Block, CursorQuery, DataType, Datum, Expression, ForSource, ParseMode, Statement};
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
        Datum::Cursor {
            variable,
            query,
            args,
            options,
        } => {
            let mut v = datum(&Datum::Variable(variable.clone()), id);
            let cursor = &mut v["PLpgSQL_var"];
            cursor["cursor_explicit_expr"] = expression(query);
            let arg = args.map_or(-1, |id| id as i32);
            if arg != 0 {
                cursor["cursor_explicit_argrow"] = json!(arg);
            }
            cursor["cursor_options"] = json!(options);
            v
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
    if !b.exceptions.is_empty() {
        let handlers: Vec<_> = b
            .exceptions
            .iter()
            .map(|handler| {
                let conditions: Vec<_> = handler
                    .conditions
                    .iter()
                    .map(|name| json!({"PLpgSQL_condition":{"condname":name}}))
                    .collect();
                let mut item = json!({"conditions":conditions});
                statements(&mut item, "action", &handler.action);
                json!({"PLpgSQL_exception": item})
            })
            .collect();
        v["exceptions"] = json!({"PLpgSQL_exception_block":{"exc_list":handlers}});
    }
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

fn open_cursor(lineno: usize, cursor: usize, options: i32, query: &CursorQuery) -> Value {
    let mut v = json!({"lineno":lineno,"cursor_options":options});
    if cursor > 0 {
        v["curvar"] = json!(cursor);
    }
    match query {
        CursorQuery::Bound(arg) => optional_expr(&mut v, "argquery", arg),
        CursorQuery::Static(query) => v["query"] = expression(query),
        CursorQuery::Dynamic(query, params) => {
            v["dynquery"] = expression(query);
            if !params.is_empty() {
                v["params"] = params.iter().map(expression).collect();
            }
        }
    }
    json!({"PLpgSQL_stmt_open":v})
}

fn diagnostics(lineno: usize, stacked: bool, items: &[crate::DiagnosticItem]) -> Value {
    let mut v = json!({"lineno":lineno});
    if stacked {
        v["is_stacked"] = json!(true);
    }
    v["diag_items"] = items
        .iter()
        .map(|item| {
            let mut a = json!({"kind":item.name});
            if item.target > 0 {
                a["target"] = json!(item.target);
            }
            json!({"PLpgSQL_diag_item":a})
        })
        .collect();
    json!({"PLpgSQL_stmt_getdiag":v})
}

fn case(
    lineno: usize,
    operand: &Option<Expression>,
    datum: usize,
    branches: &[crate::Elsif],
    otherwise: &Option<Vec<Statement>>,
) -> Value {
    let mut v = json!({"lineno":lineno});
    optional_expr(&mut v, "t_expr", operand);
    if datum != 0 {
        v["t_varno"] = json!(datum);
    }
    v["case_when_list"] = branches
        .iter()
        .map(|b| {
            let mut when = json!({"lineno":b.lineno,"expr":expression(&b.condition)});
            statements(&mut when, "stmts", &b.body);
            json!({"PLpgSQL_case_when":when})
        })
        .collect();
    if let Some(body) = otherwise {
        v["have_else"] = json!(true);
        statements(&mut v, "else_stmts", body);
    }
    json!({"PLpgSQL_stmt_case":v})
}

fn foreach(
    lineno: usize,
    label: &Option<String>,
    datum: usize,
    slice: i32,
    expr: &Expression,
    body: &[Statement],
) -> Value {
    let mut v = json!({"lineno":lineno,"expr":expression(expr)});
    if let Some(label) = label {
        v["label"] = json!(label);
    }
    if datum != 0 {
        v["varno"] = json!(datum);
    }
    if slice != 0 {
        v["slice"] = json!(slice);
    }
    statements(&mut v, "body", body);
    json!({"PLpgSQL_stmt_foreach_a":v})
}

fn for_loop(
    lineno: usize,
    label: &Option<String>,
    target: &crate::Target,
    source: &ForSource,
    body: &[Statement],
) -> Value {
    let mut v = json!({"lineno":lineno,"var":datum(&target.variable,target.id)});
    if let Some(l) = label {
        v["label"] = json!(l);
    }
    statements(&mut v, "body", body);
    let key = match source {
        ForSource::Integer {
            lower,
            upper,
            step,
            reverse,
        } => {
            v["lower"] = expression(lower);
            v["upper"] = expression(upper);
            optional_expr(&mut v, "step", step);
            if *reverse {
                v["reverse"] = json!(true);
            }
            "PLpgSQL_stmt_fori"
        }
        ForSource::Query {
            query,
            dynamic,
            params,
        } => {
            v["query"] = expression(query);
            if !params.is_empty() {
                v["params"] = params.iter().map(expression).collect();
            }
            if *dynamic {
                "PLpgSQL_stmt_dynfors"
            } else {
                "PLpgSQL_stmt_fors"
            }
        }
        ForSource::Cursor { cursor, argquery } => {
            if *cursor > 0 {
                v["curvar"] = json!(cursor);
            }
            optional_expr(&mut v, "argquery", argquery);
            "PLpgSQL_stmt_forc"
        }
    };
    json!({(key):v})
}

fn fetch(
    lineno: usize,
    cursor: usize,
    target: &Option<Box<crate::Target>>,
    dir: &crate::FetchDirection,
) -> Value {
    let mut v = json!({"lineno":lineno,"direction":dir.direction,"how_many":dir.count});
    if cursor > 0 {
        v["curvar"] = json!(cursor);
    }
    if let Some(t) = target {
        v["target"] = datum(&t.variable, t.id);
    } else {
        v["is_move"] = json!(true);
    }
    if dir.multiple {
        v["returns_multiple_rows"] = json!(true);
    }
    optional_expr(&mut v, "expr", &dir.expression);
    json!({"PLpgSQL_stmt_fetch":v})
}

fn statement(s: &Statement) -> Value {
    match s {
        Statement::GetDiagnostics {
            lineno,
            stacked,
            items,
        } => diagnostics(*lineno, *stacked, items),
        Statement::Case {
            lineno,
            operand,
            datum,
            branches,
            otherwise,
        } => case(*lineno, operand, *datum, branches, otherwise),
        Statement::Foreach {
            lineno,
            label,
            datum,
            slice,
            expression,
            body,
        } => foreach(*lineno, label, *datum, *slice, expression, body),
        Statement::Fetch {
            lineno,
            cursor,
            target,
            direction,
        } => fetch(*lineno, *cursor, target, direction),
        Statement::Close { lineno, cursor } => {
            let mut v = json!({"lineno":lineno});
            if *cursor > 0 {
                v["curvar"] = json!(cursor);
            }
            json!({"PLpgSQL_stmt_close":v})
        }
        Statement::Open {
            lineno,
            cursor,
            options,
            query,
        } => open_cursor(*lineno, *cursor, *options, query),
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
        Statement::Raise {
            lineno,
            level,
            condition,
            message,
            params,
            options,
        } => {
            let mut v = json!({"lineno":lineno, "elog_level":level});
            if let Some(name) = condition {
                v["condname"] = json!(name);
            }
            if let Some(message) = message {
                v["message"] = json!(message);
            }
            if !params.is_empty() {
                v["params"] = params.iter().map(expression).collect();
            }
            if !options.is_empty() {
                v["options"] = options.iter().map(|(ty,e)|json!({"PLpgSQL_raise_option":{"opt_type":ty,"expr":expression(e)}})).collect();
            }
            json!({"PLpgSQL_stmt_raise":v})
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
        Statement::For {
            lineno,
            label,
            target,
            source,
            body,
        } => for_loop(*lineno, label, target, source, body),
        Statement::Execute {
            lineno,
            query,
            target,
            params,
        } => {
            let mut v = json!({"lineno":lineno,"query":expression(query)});
            if let Some(t) = target {
                v["into"] = json!(true);
                if t.strict {
                    v["strict"] = json!(true);
                }
                v["target"] = datum(&t.variable, t.id);
            }
            if !params.is_empty() {
                v["params"] = params.iter().map(expression).collect();
            }
            json!({"PLpgSQL_stmt_dynexecute":v})
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

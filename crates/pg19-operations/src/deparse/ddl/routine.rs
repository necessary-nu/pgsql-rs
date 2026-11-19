// Routine, trigger and rule statements use the spelling from postgres_deparse.c.
use super::{ident, literal, named, names, object, relation, required, Error, Plan, Result};
use pg19_nodes::*;

fn string(n: &Node) -> Result<&str> {
    match n {
        Node::String(s) => Ok(s),
        _ => Err(Error::InvalidField("routine string")),
    }
}
fn quote_body(text: &str, is_do: bool) -> Result<String> {
    if text.contains('\0') {
        return Err(Error::InvalidField("routine source"));
    }
    // A suffix of the body can form a closing tag together with its delimiter.
    if !text.contains("$$") && !text.ends_with('$') {
        Ok(["$$", text, "$$"].concat())
    } else if is_do && !text.contains("$outer$") && !text.ends_with("$outer") {
        Ok(format!("$outer${text}$outer$"))
    } else {
        literal(text)
    }
}
fn language(n: &Node) -> Result<String> {
    let name = string(n)?;
    if name.is_empty() || name.len() > 63 {
        literal(name)
    } else {
        ident(name)
    }
}
fn common<'a>(key: &str, arg: &'a Node, p: &mut Plan<'a>) -> Result<()> {
    match (key, arg) {
        ("strict", Node::Boolean(true)) => p.text("RETURNS NULL ON NULL INPUT"),
        ("strict", Node::Boolean(false)) => p.text("CALLED ON NULL INPUT"),
        ("volatility", Node::String(s)) => p.text(match s.as_str() {
            "immutable" => "IMMUTABLE",
            "stable" => "STABLE",
            "volatile" => "VOLATILE",
            _ => return Err(Error::InvalidField("routine volatility")),
        }),
        ("security", Node::Boolean(definer)) => p.text(if *definer {
            "SECURITY DEFINER"
        } else {
            "SECURITY INVOKER"
        }),
        ("leakproof", Node::Boolean(leakproof)) => p.text(if *leakproof {
            "LEAKPROOF"
        } else {
            "NOT LEAKPROOF"
        }),
        ("cost" | "rows", n @ (Node::Integer(_) | Node::Float(_))) => {
            p.text(if key == "cost" { "COST " } else { "ROWS " });
            p.text(super::super::option_value(n)?);
        }
        ("support", Node::List(values)) => {
            p.text("SUPPORT ");
            p.text(names(values, ".")?);
        }
        ("set", Node::VariableSetStmt(_)) => p.statement(arg),
        ("parallel", v) => {
            p.text("PARALLEL ");
            p.text(ident(string(v)?)?);
        }
        _ => return Err(Error::InvalidField("routine option")),
    }
    Ok(())
}
fn options<'a>(opts: &'a [Node], p: &mut Plan<'a>) -> Result<()> {
    for opt in opts {
        let Node::DefElem(opt) = opt else {
            return Err(Error::InvalidField("routine options"));
        };
        let key = opt
            .defname
            .as_deref()
            .ok_or(Error::InvalidField("DefElem.defname"))?;
        let arg = required(&opt.arg, "DefElem.arg")?;
        p.text(" ");
        match (key, arg) {
            ("as", Node::List(body)) => {
                p.text("AS ");
                for (i, s) in body.iter().enumerate() {
                    if i > 0 {
                        p.text(", ");
                    }
                    p.text(quote_body(string(s)?, false)?);
                }
            }
            ("language", l) => {
                p.text("LANGUAGE ");
                p.text(language(l)?);
            }
            ("transform", Node::List(types)) => {
                p.text("TRANSFORM ");
                for (j, t) in types.iter().enumerate() {
                    let Node::TypeName(t) = t else {
                        return Err(Error::InvalidField("TRANSFORM type"));
                    };
                    if j > 0 {
                        p.text(", ");
                    }
                    p.text("FOR TYPE ");
                    p.typename(t);
                }
            }
            ("window", Node::Boolean(true)) => p.text("WINDOW"),
            _ => common(key, arg, p)?,
        }
    }
    Ok(())
}
fn params<'a>(args: &'a [Node], table: bool, p: &mut Plan<'a>) -> Result<()> {
    let mut first = true;
    for n in args {
        let Node::FunctionParameter(arg) = n else {
            return Err(Error::InvalidField("CreateFunctionStmt.parameters"));
        };
        if (arg.mode == FunctionParameterMode::FUNC_PARAM_TABLE) != table {
            continue;
        }
        if !first {
            p.text(", ");
        }
        first = false;
        object::parameter(arg, p)?;
    }
    Ok(())
}
pub(super) fn create<'a>(f: &'a CreateFunctionStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("CREATE ");
    if f.replace {
        p.text("OR REPLACE ");
    }
    p.text(if f.is_procedure {
        "PROCEDURE "
    } else {
        "FUNCTION "
    });
    p.text(names(&f.funcname, ".")?);
    p.text("(");
    params(&f.parameters, false, p)?;
    p.text(")");
    if f.parameters.iter().any(|n|matches!(n,Node::FunctionParameter(arg) if arg.mode==FunctionParameterMode::FUNC_PARAM_TABLE)){
        p.text(" RETURNS TABLE (");params(&f.parameters,true,p)?;p.text(")");
    }else if let Some(t)=&f.return_type{p.text(" RETURNS ");p.typename(t);}
    options(&f.options, p)?;
    if let Some(body) = f.sql_body.as_deref() {
        match body {
            Node::ReturnStmt(_) => {
                p.text(" ");
                p.statement(body);
            }
            Node::List(rows) => {
                p.text(" BEGIN ATOMIC ");
                match rows.as_slice() {
                    [Node::List(stmts)] => {
                        for n in stmts.iter() {
                            p.statement(n);
                            p.text("; ");
                        }
                    }
                    [Node::Null] => {}
                    _ => return Err(Error::InvalidField("CreateFunctionStmt.sql_body")),
                }
                p.text("END");
            }
            _ => return Err(Error::InvalidField("CreateFunctionStmt.sql_body")),
        }
    }
    Ok(())
}
pub(super) fn alter<'a>(f: &'a AlterFunctionStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text(match f.objtype {
        ObjectType::OBJECT_FUNCTION => "ALTER FUNCTION ",
        ObjectType::OBJECT_PROCEDURE => "ALTER PROCEDURE ",
        ObjectType::OBJECT_ROUTINE => "ALTER ROUTINE ",
        _ => return Err(Error::InvalidField("AlterFunctionStmt.objtype")),
    });
    object::signature(f.objtype, required(&f.func, "AlterFunctionStmt.func")?, p)?;
    options(&f.actions, p)
}
pub(super) fn do_block<'a>(s: &'a DoStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("DO");
    for n in &s.args {
        let Node::DefElem(opt) = n else {
            return Err(Error::InvalidField("DoStmt.args"));
        };
        let arg = required(&opt.arg, "DefElem.arg")?;
        match opt.defname.as_deref() {
            Some("language") => {
                p.text(" LANGUAGE ");
                p.text(language(arg)?);
            }
            Some("as") => {
                p.text(" ");
                p.text(quote_body(string(arg)?, true)?);
            }
            _ => return Err(Error::InvalidField("DO option")),
        }
    }
    Ok(())
}
pub(super) fn trigger<'a>(t: &'a CreateTrigStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("CREATE ");
    if t.replace {
        p.text("OR REPLACE ");
    }
    if t.isconstraint {
        p.text("CONSTRAINT ");
    }
    p.text("TRIGGER ");
    p.text(named(&t.trigname, "CreateTrigStmt.trigname")?);
    p.text(match t.timing {
        0 => " AFTER ",
        2 => " BEFORE ",
        64 => " INSTEAD OF ",
        _ => return Err(Error::InvalidField("CreateTrigStmt.timing")),
    });
    let mut first = true;
    for (flag, word) in [
        (4, "INSERT"),
        (8, "DELETE"),
        (16, "UPDATE"),
        (32, "TRUNCATE"),
    ] {
        if t.events & flag == 0 {
            continue;
        }
        if !first {
            p.text(" OR ");
        }
        first = false;
        p.text(word);
        if flag == 16 && !t.columns.is_empty() {
            p.text(" OF ");
            p.text(names(&t.columns, ", ")?);
        }
    }
    if first || t.events & !60 != 0 {
        return Err(Error::InvalidField("CreateTrigStmt.events"));
    }
    p.text(" ON ");
    relation(required(&t.relation, "CreateTrigStmt.relation")?, p, false)?;
    if !t.transition_rels.is_empty() {
        p.text(" REFERENCING");
    }
    for n in &t.transition_rels {
        let Node::TriggerTransition(tr) = n else {
            return Err(Error::InvalidField("CreateTrigStmt.transitionRels"));
        };
        p.text(if tr.is_new { " NEW " } else { " OLD " });
        p.text(if tr.is_table { "TABLE " } else { "ROW " });
        p.text(named(&tr.name, "TriggerTransition.name")?);
    }
    if let Some(r) = &t.constrrel {
        p.text(" FROM ");
        relation(r, p, false)?;
    }
    if t.deferrable {
        p.text(" DEFERRABLE");
    }
    if t.initdeferred {
        p.text(" INITIALLY DEFERRED");
    }
    if t.row {
        p.text(" FOR EACH ROW");
    }
    if let Some(w) = &t.when_clause {
        p.text(" WHEN (");
        p.expr(w);
        p.text(")");
    }
    p.text(" EXECUTE FUNCTION ");
    p.text(names(&t.funcname, ".")?);
    p.text("(");
    for (j, s) in t.args.iter().enumerate() {
        if j > 0 {
            p.text(", ");
        }
        p.text(literal(string(s)?)?);
    }
    p.text(")");
    Ok(())
}
pub(super) fn rule<'a>(r: &'a RuleStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("CREATE ");
    if r.replace {
        p.text("OR REPLACE ");
    }
    p.text("RULE ");
    p.text(named(&r.rulename, "RuleStmt.rulename")?);
    p.text(" AS ON ");
    p.text(match r.event {
        CmdType::CMD_SELECT => "SELECT",
        CmdType::CMD_UPDATE => "UPDATE",
        CmdType::CMD_INSERT => "INSERT",
        CmdType::CMD_DELETE => "DELETE",
        CmdType::CMD_MERGE => "MERGE",
        _ => return Err(Error::InvalidField("RuleStmt.event")),
    });
    p.text(" TO ");
    relation(required(&r.relation, "RuleStmt.relation")?, p, false)?;
    if let Some(w) = &r.where_clause {
        p.clause("WHERE ");
        p.expr(w);
    }
    p.text(" DO ");
    if r.instead {
        p.text("INSTEAD ");
    }
    match r.actions.as_slice() {
        [] => p.text("NOTHING"),
        [one] => p.statement(one),
        actions => {
            p.text("(");
            for (i, n) in actions.iter().enumerate() {
                if i > 0 {
                    p.text("; ");
                }
                p.statement(n);
            }
            p.text(")");
        }
    }
    Ok(())
}

pub(super) fn event<'a>(t: &'a CreateEventTrigStmt, p: &mut Plan<'a>) -> Result<()> {
    p.text("CREATE EVENT TRIGGER ");
    p.text(named(&t.trigname, "CreateEventTrigStmt.trigname")?);
    p.text(" ON ");
    p.text(named(&t.eventname, "CreateEventTrigStmt.eventname")?);
    for (i, n) in t.whenclause.iter().enumerate() {
        let Node::DefElem(e) = n else {
            return Err(Error::InvalidField("event trigger condition"));
        };
        p.text(if i == 0 { " WHEN " } else { " AND " });
        p.text(named(&e.defname, "DefElem.defname")?);
        p.text(" IN (");
        let Node::List(tags) = required(&e.arg, "DefElem.arg")? else {
            return Err(Error::InvalidField("event trigger values"));
        };
        for (j, tag) in tags.iter().enumerate() {
            if j > 0 {
                p.text(", ");
            }
            p.text(literal(string(tag)?)?);
        }
        p.text(")");
    }
    p.text(" EXECUTE FUNCTION ");
    p.text(names(&t.funcname, ".")?);
    p.text("()");
    Ok(())
}
pub(super) fn alter_event(t: &AlterEventTrigStmt, p: &mut Plan<'_>) -> Result<()> {
    p.text("ALTER EVENT TRIGGER ");
    p.text(named(&t.trigname, "AlterEventTrigStmt.trigname")?);
    p.text(match t.tgenabled {
        b'O' => " ENABLE",
        b'R' => " ENABLE REPLICA",
        b'A' => " ENABLE ALWAYS",
        b'D' => " DISABLE",
        _ => return Err(Error::InvalidField("AlterEventTrigStmt.tgenabled")),
    });
    Ok(())
}

pub(super) fn statement<'a>(n: &'a Node, p: &mut Plan<'a>) -> Result<bool> {
    match n {
        Node::CreateFunctionStmt(s) => create(s, p)?,
        Node::AlterFunctionStmt(s) => alter(s, p)?,
        Node::DoStmt(s) => do_block(s, p)?,
        Node::CreateEventTrigStmt(s) => event(s, p)?,
        Node::AlterEventTrigStmt(s) => alter_event(s, p)?,
        Node::CreateTrigStmt(s) => trigger(s, p)?,
        Node::RuleStmt(s) => rule(s, p)?,
        Node::CallStmt(s) => {
            p.text("CALL ");
            super::super::expr::function(required(&s.funccall, "CallStmt.funccall")?, p)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

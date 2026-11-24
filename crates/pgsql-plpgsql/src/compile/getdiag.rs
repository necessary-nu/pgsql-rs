use super::*;
use crate::{Datum, DiagnosticItem};

const ITEMS: &[(u16, &str, &str)] = &[
    (token::K_ROW_COUNT, "row_count", "ROW_COUNT"),
    (token::K_PG_ROUTINE_OID, "pg_routine_oid", "PG_ROUTINE_OID"),
    (token::K_PG_CONTEXT, "pg_context", "PG_CONTEXT"),
    (
        token::K_PG_EXCEPTION_DETAIL,
        "pg_exception_detail",
        "PG_EXCEPTION_DETAIL",
    ),
    (
        token::K_PG_EXCEPTION_HINT,
        "pg_exception_hint",
        "PG_EXCEPTION_HINT",
    ),
    (
        token::K_PG_EXCEPTION_CONTEXT,
        "pg_exception_context",
        "PG_EXCEPTION_CONTEXT",
    ),
    (token::K_COLUMN_NAME, "column_name", "COLUMN_NAME"),
    (
        token::K_CONSTRAINT_NAME,
        "constraint_name",
        "CONSTRAINT_NAME",
    ),
    (
        token::K_PG_DATATYPE_NAME,
        "pg_datatype_name",
        "PG_DATATYPE_NAME",
    ),
    (token::K_MESSAGE_TEXT, "message_text", "MESSAGE_TEXT"),
    (token::K_TABLE_NAME, "table_name", "TABLE_NAME"),
    (token::K_SCHEMA_NAME, "schema_name", "SCHEMA_NAME"),
    (
        token::K_RETURNED_SQLSTATE,
        "returned_sqlstate",
        "RETURNED_SQLSTATE",
    ),
];

impl<S: EmbeddedSql> Compiler<'_, S> {
    pub(super) fn getdiag(
        &mut self,
        lhs: &str,
        prod: &str,
        rhs: &mut Reduction<Reduced>,
        s: &mut BodyScanner,
    ) -> Result<Reduced, CompileError> {
        Ok(match lhs {
            "getdiag_area_opt" => Reduced::Number((prod == "K_STACKED") as i32),
            "getdiag_target" => {
                let t = rhs.take(1).token();
                let name = t.names.join(".");
                let id = t.datum.ok_or_else(|| {
                    Error::at(
                        s.source(),
                        t.span.start,
                        format!("\"{name}\" is not a known variable"),
                        "42601",
                    )
                })?;
                if matches!(&self.env.datums[id], Datum::Variable(v) if v.datatype==DataType::Record)
                    || s.peek_raw(0)?.id == 91
                {
                    return Err(Error::at(
                        s.source(),
                        t.span.start,
                        format!("\"{name}\" is not a scalar variable"),
                        "42601",
                    )
                    .into());
                }
                let v = self.env.variable(id);
                if v.constant {
                    return Err(Error::at(
                        s.source(),
                        t.span.start,
                        format!("variable \"{}\" is declared CONSTANT", v.name),
                        "22005",
                    )
                    .into());
                }
                Reduced::Datum(id)
            }
            "getdiag_item" => {
                let t = s.next_token(self)?;
                let id = ITEMS
                    .iter()
                    .position(|(id, name, _)| t.is_keyword(*id, name))
                    .ok_or_else(|| s.error("unrecognized GET DIAGNOSTICS item"))?;
                Reduced::Number(id as i32)
            }
            "getdiag_list_item" => {
                let target = rhs.take(1).datum();
                let Reduced::Number(i) = rhs.take(3) else {
                    unreachable!()
                };
                Reduced::Diags(vec![DiagnosticItem {
                    target,
                    name: ITEMS[i as usize].2,
                }])
            }
            "getdiag_list" if prod == "getdiag_list_item" => rhs.take(1),
            "getdiag_list" => {
                let Reduced::Diags(mut items) = rhs.take(1) else {
                    unreachable!()
                };
                let Reduced::Diags(tail) = rhs.take(3) else {
                    unreachable!()
                };
                items.extend(tail);
                Reduced::Diags(items)
            }
            "stmt_getdiag" => {
                let lineno = s.line(rhs.location(1));
                let stacked = matches!(rhs.value(2), Reduced::Number(1));
                let Reduced::Diags(items) = rhs.take(4) else {
                    unreachable!()
                };
                for item in &items {
                    if item.name != "PG_CONTEXT"
                        && matches!(item.name, "ROW_COUNT" | "PG_ROUTINE_OID") == stacked
                    {
                        let area = if stacked { "STACKED" } else { "CURRENT" };
                        return Err(Error::at(
                            s.source(),
                            rhs.location(1),
                            format!(
                                "diagnostics item {} is not allowed in GET {area} DIAGNOSTICS",
                                item.name
                            ),
                            "42601",
                        )
                        .into());
                    }
                }
                Reduced::Statement(Statement::GetDiagnostics {
                    lineno,
                    stacked,
                    items,
                })
            }
            _ => unreachable!(),
        })
    }
}

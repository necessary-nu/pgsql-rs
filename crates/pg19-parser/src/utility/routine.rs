//! Function signatures, TABLE return columns and SQL routine bodies from gram.y.
use crate::{call, records::optional_node, types, value::Value, Failure};
use pg19_engine::Reduction;
use pg19_nodes::*;

fn parameter(node: &Node) -> Result<&FunctionParameter, Failure> {
    match node {
        Node::FunctionParameter(param) => Ok(param),
        _ => Err("expected function parameter".into()),
    }
}

pub(super) fn signature(
    rhs: &mut Reduction<'_, Value>,
    qualified_only: bool,
) -> Result<Value, Failure> {
    if qualified_only {
        return Ok(Value::Node(
            ObjectWithArgs {
                objname: call::function_name(true, rhs)?.list()?,
                args_unspecified: true,
                ..Default::default()
            }
            .into(),
        ));
    }
    let objfuncargs = rhs.take(2).list()?;
    let mut objargs = Vec::new();
    for arg in &objfuncargs {
        let param = parameter(arg)?;
        if !matches!(
            param.mode,
            FunctionParameterMode::FUNC_PARAM_OUT | FunctionParameterMode::FUNC_PARAM_TABLE
        ) {
            objargs.push(
                param
                    .arg_type
                    .clone()
                    .ok_or("expected argument type")?
                    .into(),
            );
        }
    }
    Ok(Value::Node(
        ObjectWithArgs {
            objname: rhs.take(1).list()?,
            objargs,
            objfuncargs,
            args_unspecified: false,
        }
        .into(),
    ))
}

pub(super) fn table_function(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let mut parameters = rhs.take(5).list()?;
    for input in &parameters {
        let param = parameter(input)?;
        if !matches!(
            param.mode,
            FunctionParameterMode::FUNC_PARAM_DEFAULT
                | FunctionParameterMode::FUNC_PARAM_IN
                | FunctionParameterMode::FUNC_PARAM_VARIADIC
        ) {
            return Err(Failure::At(
                "OUT and INOUT arguments aren't allowed in TABLE functions".to_owned(),
                "42601",
                param.location,
            ));
        }
    }
    let columns = rhs.take(9).list()?;
    let mut result = if let [column] = &columns[..] {
        parameter(column)?
            .arg_type
            .clone()
            .ok_or("expected table result type")?
    } else {
        Box::new(types::builtin("record", vec![], rhs.location(7)))
    };
    result.setof = true;
    result.location = rhs.location(7);
    parameters.extend(columns);
    Ok(Value::Node(
        CreateFunctionStmt {
            replace: rhs.take(2).integer()? != 0,
            funcname: rhs.take(4).list()?,
            parameters,
            return_type: Some(result),
            options: rhs.take(11).list()?,
            sql_body: optional_node(rhs.take(12))?,
            ..Default::default()
        }
        .into(),
    ))
}

pub(super) fn percent_type(rhs: &mut Reduction<'_, Value>, setof: bool) -> Result<Value, Failure> {
    let index = if setof { 2 } else { 1 };
    let mut names = vec![Node::String(rhs.take(index).text()?)];
    names.extend(rhs.take(index + 1).list()?);
    let mut ty = types::named(names, vec![], rhs.location(index));
    ty.setof = setof;
    ty.pct_type = true;
    Ok(Value::Node(ty.into()))
}

pub(super) fn body(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let mut list = rhs.take(1).list()?;
    if let Some(statement) = optional_node(rhs.take(2))? {
        list.push(*statement);
    }
    Ok(Value::List(list))
}

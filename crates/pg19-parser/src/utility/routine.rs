//! Function signatures, TABLE return columns and SQL routine bodies from gram.y.
use crate::raw::*;
use crate::{
    call,
    records::{optional_node, record},
    types,
    value::Value,
    Failure,
};
use pg19_engine::Reduction;

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
        return Ok(Value::from_node(
            ObjectWithArgs {
                objname: call::function_name(true, rhs)?.list()?,
                args_unspecified: true,
                ..Default::default()
            }
            .into(),
        ));
    }
    object_signature(rhs.take(1).list()?, rhs.take(2).list()?)
}

fn object_signature(objname: Vec<Node>, objfuncargs: Vec<Node>) -> Result<Value, Failure> {
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
    Ok(Value::from_node(
        ObjectWithArgs {
            objname,
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
    Ok(Value::from_node(
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
    Ok(Value::from_node(ty.into()))
}

pub(super) fn body(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let mut list = rhs.take(1).list()?;
    if let Some(statement) = optional_node(rhs.take(2))? {
        list.push(statement.into_inner());
    }
    Ok(Value::from_list(list))
}

pub(super) fn transform(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let [from, to]: [Node; 2] = rhs
        .take(9)
        .list()?
        .try_into()
        .map_err(|_| Failure::Value("expected a pair of transform functions"))?;
    let function = |node| match node {
        Node::Null => Ok(None),
        Node::ObjectWithArgs(f) => Ok(Some(f.into_box())),
        _ => Err(Failure::Value("expected a transform function signature")),
    };
    Ok(Value::from_node(
        CreateTransformStmt {
            replace: rhs.take(2).integer()? != 0,
            type_name: record!(rhs.take(5), TypeName),
            lang: Some(rhs.take(7).text()?),
            fromsql: function(from)?,
            tosql: function(to)?,
        }
        .into(),
    ))
}

pub(crate) fn aggregate_signature(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let args = rhs.take(2).list()?;
    let first = args
        .into_iter()
        .next()
        .ok_or("expected aggregate arguments")?;
    let params = match first {
        Node::Null => Vec::new(),
        Node::List(items) => items.into_vec(),
        _ => return Err("expected aggregate parameter list".into()),
    };
    object_signature(rhs.take(1).list()?, params)
}

pub(crate) fn aggregate_arg(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let arg = rhs.take(1).node()?;
    if !matches!(
        parameter(&arg)?.mode,
        FunctionParameterMode::FUNC_PARAM_DEFAULT
            | FunctionParameterMode::FUNC_PARAM_IN
            | FunctionParameterMode::FUNC_PARAM_VARIADIC
    ) {
        return Err(Failure::At(
            "aggregates cannot have output arguments".to_owned(),
            "0A000",
            rhs.location(1),
        ));
    }
    Ok(Value::from_node(arg))
}

pub(crate) fn ordered_args(rhs: &mut Reduction<'_, Value>) -> Result<Value, Failure> {
    let mut direct = rhs.take(2).list()?;
    let ordered = rhs.take(5).list()?;
    let count = i32::try_from(direct.len()).map_err(|_| "too many direct aggregate arguments")?;
    let last = parameter(direct.last().ok_or("expected direct aggregate parameter")?)?;
    if last.mode == FunctionParameterMode::FUNC_PARAM_VARIADIC {
        let first = parameter(
            ordered
                .first()
                .ok_or("expected ordered aggregate parameter")?,
        )?;
        let direct_type = last.arg_type.as_deref().ok_or("expected direct type")?;
        let ordered_type = first.arg_type.as_deref().ok_or("expected ordered type")?;
        if ordered.len() != 1
            || first.mode != FunctionParameterMode::FUNC_PARAM_VARIADIC
            || !direct_type
                .pg_equal(ordered_type)
                .map_err(|_| "unsupported PostgreSQL equality tag")?
        {
            return Err(Failure::At("an ordered-set aggregate with a VARIADIC direct argument must have one VARIADIC aggregated argument of the same data type".to_owned(),"0A000",first.location));
        }
    } else {
        direct.extend(ordered);
    }
    Ok(Value::from_list(vec![direct.into(), Node::Integer(count)]))
}

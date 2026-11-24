use super::*;

fn scalar(name: &str, value: Value) -> Value {
    let mut m = DynamicMessage::new(schema(name));
    m.set_field_by_number(1, value);
    Value::Message(m)
}

fn sequence<E>(name: &str, items: &[E], wrap: impl Fn(&E) -> Result<Value>) -> Result<Value> {
    Ok(scalar(
        name,
        Value::List(items.iter().map(wrap).collect::<Result<_>>()?),
    ))
}

pub(super) fn pack_special(x: &Node, d: Depth) -> Result<Value> {
    Ok(match x {
        Node::Integer(v) => scalar("Integer", Value::I32(*v)),
        Node::Float(v) => scalar("Float", Value::String(v.clone())),
        Node::String(v) => scalar("String", Value::String(v.clone())),
        Node::BitString(v) => scalar("BitString", Value::String(v.clone())),
        Node::Boolean(v) => scalar("Boolean", Value::Bool(*v)),
        Node::List(items) => sequence("List", items, |v| v.to_message(d))?,
        Node::IntList(items) => sequence("IntList", items, |v| Node::Integer(*v).to_message(d))?,
        Node::OidList(items) => {
            sequence("OidList", items, |v| Node::Integer(*v as i32).to_message(d))?
        }
        _ => return Err(ProtobufError::Invalid("unsupported scalar node".into())),
    })
}

pub(super) fn unpack_special(name: &str, v: Value, d: Depth) -> Result<Node> {
    let mut m = message(v)?;
    Ok(match name {
        "Integer" => Node::Integer(field(&m, 1).as_i32().unwrap()),
        "Boolean" => Node::Boolean(field(&m, 1).as_bool().unwrap()),
        "Float" => Node::Float(field(&m, 1).as_str().unwrap().to_owned()),
        "String" => Node::String(field(&m, 1).as_str().unwrap().to_owned()),
        "BitString" => Node::BitString(field(&m, 1).as_str().unwrap().to_owned()),
        "List" => Node::List(
            entries(&mut m, 1)?
                .into_iter()
                .map(|v| Node::from_message(v, d))
                .collect::<Result<_>>()?,
        ),
        "IntList" | "OidList" => {
            let values = entries(&mut m, 1)?
                .into_iter()
                .map(|v| match Node::from_message(v, d)? {
                    Node::Integer(i) => Ok(i),
                    _ => Err(ProtobufError::Invalid("non-integer in numeric list".into())),
                })
                .collect::<Result<Vec<_>>>()?;
            if name == "IntList" {
                Node::IntList(values)
            } else {
                Node::OidList(values.into_iter().map(|v| v as u32).collect())
            }
        }
        _ => return Err(ProtobufError::Unsupported(name.into())),
    })
}

// [spec:pgq:req:tree.literal-variants]
impl Record for AConst {
    const NAME: &'static str = "A_Const";
    fn write(&self, m: &mut DynamicMessage, _: Depth) -> Result<()> {
        let (number, value) = match &self.val {
            Literal::Null => (10, Value::Bool(true)),
            Literal::Integer(v) => (1, scalar("Integer", Value::I32(*v))),
            Literal::Float(v) => (2, scalar("Float", Value::String(v.clone()))),
            Literal::Boolean(v) => (3, scalar("Boolean", Value::Bool(*v))),
            Literal::String(v) => (4, scalar("String", Value::String(v.clone()))),
            Literal::BitString(v) => (5, scalar("BitString", Value::String(v.clone()))),
        };
        m.set_field_by_number(number, value);
        m.set_field_by_number(11, Value::I32(self.location.as_pg()));
        Ok(())
    }
    fn read(mut m: DynamicMessage, _: Depth) -> Result<Self> {
        let location = Location::from_pg(field(&m, 11).as_i32().unwrap())
            .ok_or_else(|| ProtobufError::Invalid("constant location".into()))?;
        let isnull = field(&m, 10).as_bool().unwrap();
        let arm = (1..=5).find_map(|n| m.take_field_by_number(n).map(|v| (n, v)));
        let val = match (isnull, arm) {
            (true, None) => Literal::Null,
            (false, Some((n, v))) => {
                let inner = field(&message(v)?, 1);
                match n {
                    1 => Literal::Integer(inner.as_i32().unwrap()),
                    2 => Literal::Float(inner.as_str().unwrap().into()),
                    3 => Literal::Boolean(inner.as_bool().unwrap()),
                    4 => Literal::String(inner.as_str().unwrap().into()),
                    5 => Literal::BitString(inner.as_str().unwrap().into()),
                    _ => unreachable!(),
                }
            }
            _ => {
                return Err(ProtobufError::Invalid(
                    "A_Const requires a value or isnull".into(),
                ))
            }
        };
        Ok(Self { val, location })
    }
}

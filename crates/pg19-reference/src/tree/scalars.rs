use super::*;
use std::{
    collections::BTreeSet,
    fmt::{Display, Write},
};

fn token(out: &mut String, s: &str) {
    let bytes = s.as_bytes();
    match bytes {
        [] => {
            out.push_str("\"\"");
            return;
        }
        [b'<' | b'"' | b'0'..=b'9', ..] | [b'+' | b'-', b'0'..=b'9' | b'.', ..] => out.push('\\'),
        _ => {}
    }
    for c in s.chars() {
        if matches!(c, ' ' | '\n' | '\t' | '(' | ')' | '{' | '}' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
}

pub(super) fn quoted(out: &mut String, s: &str) {
    out.push('"');
    if !s.is_empty() {
        token(out, s);
    }
    out.push('"');
}

fn numbers<T: Display>(out: &mut String, prefix: char, items: impl IntoIterator<Item = T>) {
    out.push('(');
    out.push(prefix);
    for item in items {
        write!(out, " {item}").unwrap();
    }
    out.push(')');
}

pub(super) fn node<'a>(n: &'a Node, w: &mut Writer<'a>) -> Result<()> {
    match n {
        Node::Null => w.out.push_str("<>"),
        Node::Integer(v) => write!(&mut w.out, "{v}").unwrap(),
        Node::Float(v) => w.out.push_str(v),
        Node::Boolean(v) => w.out.push_str(if *v { "true" } else { "false" }),
        Node::String(v) => quoted(&mut w.out, v),
        Node::BitString(v) => token(&mut w.out, v),
        Node::List(v) => w.sequence(v, false),
        Node::IntList(v) => numbers(&mut w.out, 'i', v),
        Node::OidList(v) => numbers(&mut w.out, 'o', v),
        Node::XidList(v) => numbers(&mut w.out, 'x', v),
        _ => unreachable!("records have their own writer"),
    }
    Ok(())
}

pub(super) fn literal(v: &Literal, out: &mut String) {
    match v {
        Literal::Integer(i) => write!(out, "{i}").unwrap(),
        Literal::Boolean(v) => out.push_str(if *v { "true" } else { "false" }),
        Literal::Float(v) => out.push_str(v),
        Literal::BitString(v) => token(out, v),
        Literal::String(v) => quoted(out, v),
        Literal::Null => unreachable!("null constants use the A_CONST NULL arm"),
    }
}

macro_rules! decimal {
    ($($ty:ty),+) => { $(impl Emit for $ty {
        fn expand(&self, w: &mut Writer<'_>) -> Result<()> { write!(&mut w.out, "{self}").unwrap(); Ok(()) }
    })+ };
}
decimal!(i16, i32, i64, u32, u64, bool);

impl Emit for Location {
    fn expand(&self, w: &mut Writer<'_>) -> Result<()> {
        write!(&mut w.out, "{}", self.as_pg()).unwrap();
        Ok(())
    }
}

impl Emit for u8 {
    fn expand(&self, w: &mut Writer<'_>) -> Result<()> {
        if *self == 0 {
            w.out.push_str("<>");
        } else if self.is_ascii() {
            token(&mut w.out, &char::from(*self).to_string());
        } else {
            return Err(TreeError::NonAsciiChar(*self));
        }
        Ok(())
    }
}

impl Emit for Option<String> {
    fn expand(&self, w: &mut Writer<'_>) -> Result<()> {
        match self {
            Some(s) => token(&mut w.out, s),
            None => w.out.push_str("<>"),
        }
        Ok(())
    }
}

impl Emit for BTreeSet<u32> {
    fn expand(&self, w: &mut Writer<'_>) -> Result<()> {
        numbers(&mut w.out, 'b', self);
        Ok(())
    }
}

impl Emit for f64 {
    fn expand(&self, w: &mut Writer<'_>) -> Result<()> {
        if self.is_infinite() {
            w.out.push_str(if self.is_sign_negative() {
                "-Infinity"
            } else {
                "Infinity"
            });
        } else if self.is_nan() {
            w.out.push_str("NaN");
        } else {
            let scientific = format!("{self:e}");
            let (mantissa, exponent) = scientific.split_once('e').unwrap();
            let exponent: i32 = exponent.parse().unwrap();
            if (-4..15).contains(&exponent) {
                write!(w.out, "{self}").unwrap();
            } else {
                write!(w.out, "{mantissa}e{exponent:+03}").unwrap();
            }
        }
        Ok(())
    }
}

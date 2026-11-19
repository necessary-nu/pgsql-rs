use crate::{DataType, Datum, Namespace, Resolved, Variable};

#[derive(Default)]
struct Scope {
    label: Option<String>,
    variables: Vec<usize>,
}

#[derive(Default)]
pub(crate) struct Environment {
    scopes: Vec<Scope>,
    pub datums: Vec<Datum>,
}

impl Environment {
    pub fn push(&mut self, label: Option<String>) {
        self.scopes.push(Scope {
            label,
            variables: Vec::new(),
        });
    }

    pub fn pop(&mut self) {
        self.scopes.pop().unwrap();
    }

    pub fn declare(&mut self, variable: Variable) {
        self.scopes
            .last_mut()
            .unwrap()
            .variables
            .push(self.datums.len());
        self.datums.push(Datum::Variable(variable));
    }

    pub fn local(&self, name: &str) -> bool {
        self.scopes
            .last()
            .unwrap()
            .variables
            .iter()
            .any(|&i| self.variable(i).name == name)
    }

    pub fn variable(&self, id: usize) -> &Variable {
        match &self.datums[id] {
            Datum::Variable(v) => v,
            Datum::RecordField { parent, .. } => self.variable(*parent),
        }
    }

    fn record_field(&mut self, record: usize, name: &str) -> usize {
        if let Some(index) = self.datums.iter().position(|d| matches!(d, Datum::RecordField { name: field, parent } if *parent == record && field == name)) {
            return index;
        }
        let index = self.datums.len();
        self.datums.push(Datum::RecordField {
            name: name.to_owned(),
            parent: record,
        });
        index
    }

    fn find(&self, names: &[String]) -> Option<(usize, usize)> {
        for scope in self.scopes.iter().rev() {
            for used in 1..=2 {
                if used > names.len() || (used == 2 && scope.label.as_ref() != names.first()) {
                    continue;
                }
                for &id in scope.variables.iter().rev() {
                    let v = self.variable(id);
                    if v.name == names[used - 1]
                        && (used == names.len() || v.datatype == DataType::Record)
                    {
                        return Some((id, used));
                    }
                }
            }
        }
        None
    }
}

impl Namespace for Environment {
    fn lookup(&mut self, names: &[String]) -> Option<Resolved> {
        let (id, used) = self.find(names)?;
        match names.get(used) {
            Some(field) => Some(Resolved {
                datum: self.record_field(id, field),
                names_used: used + 1,
            }),
            None => Some(Resolved {
                datum: id,
                names_used: used,
            }),
        }
    }
}

//! Parameterize raw SQL constants at the byte offsets recorded by PostgreSQL.
use crate::fingerprint::fingerprint_node;
use crate::raw_walk::RawRef;
use pg19_lexer::Scanner;
use pg19_nodes::*;
use pg19_parser::{parse_statements, Error, Options};
use std::collections::{HashMap, VecDeque};
use std::fmt::Write;

fn list<'a>(jobs: &mut Vec<Job<'a>>, nodes: &'a [Node]) {
    if !nodes.is_empty() {
        jobs.push(Job::Visit(RawRef::List(nodes)));
    }
}

fn pointer<'a, T>(jobs: &mut Vec<Job<'a>>, node: Option<&'a T>)
where
    RawRef<'a>: From<&'a T>,
{
    if let Some(node) = node {
        jobs.push(Job::Visit(node.into()));
    }
}

enum Job<'a> {
    Visit(RawRef<'a>),
    StartTarget(&'a ResTarget),
    EndTarget(Option<&'a Node>),
    Group(&'a Node),
    Reuse { start: usize, ids: Vec<i64> },
    EndSelect,
}

type Targets = HashMap<u64, VecDeque<Vec<i64>>>;

#[derive(Default)]
struct State {
    locations: Vec<(usize, i64)>,
    assigned: i64,
    external: i64,
    capture: Option<Vec<i64>>,
    selects: Vec<Targets>,
    utility_only: bool,
}

fn ordinal(n: &Node) -> bool {
    matches!(
        n,
        Node::AConst(AConst {
            val: Literal::Integer(_),
            ..
        })
    )
}

// [spec:pgq:req:normalize.grouping]
fn select<'a>(stmt: &'a SelectStmt, jobs: &mut Vec<Job<'a>>) {
    list(jobs, &stmt.distinct_clause);
    pointer(jobs, stmt.into_clause.as_deref());
    for target in &stmt.target_list {
        if let Node::ResTarget(n) = target {
            jobs.push(Job::StartTarget(n));
            jobs.push(Job::EndTarget(
                n.val.as_deref().filter(|_| !stmt.group_clause.is_empty()),
            ));
        }
    }
    list(jobs, &stmt.from_clause);
    pointer(jobs, stmt.where_clause.as_deref());
    for n in stmt.group_clause.iter().filter(|n| !ordinal(n)) {
        jobs.push(Job::Group(n));
    }
    for n in &stmt.sort_clause {
        if !matches!(n, Node::SortBy(s) if s.node.as_deref().is_some_and(ordinal)) {
            jobs.push(Job::Visit(n.into()));
        }
    }
    pointer(jobs, stmt.having_clause.as_deref());
    list(jobs, &stmt.window_clause);
    list(jobs, &stmt.values_lists);
    pointer(jobs, stmt.limit_offset.as_deref());
    pointer(jobs, stmt.limit_count.as_deref());
    list(jobs, &stmt.locking_clause);
    pointer(jobs, stmt.with_clause.as_deref());
    pointer(jobs, stmt.larg.as_deref());
    pointer(jobs, stmt.rarg.as_deref());
    jobs.push(Job::EndSelect);
}

impl State {
    fn record(&mut self, loc: Location) {
        if let Some(offset) = loc.byte_offset() {
            self.assigned += 1;
            let id = -self.assigned;
            self.locations.push((offset, id));
            if let Some(refs) = &mut self.capture {
                refs.push(id);
            }
        }
    }

    // [spec:pgq:req:normalize.utility]
    fn visit<'a>(&mut self, raw: RawRef<'a>, jobs: &mut Vec<Job<'a>>) {
        match raw {
            RawRef::Constant(n) => self.record(n.location),
            RawRef::ParamRef(n) => {
                let id = i64::from(n.number);
                self.external = self.external.max(id);
                if let Some(refs) = &mut self.capture {
                    refs.push(id);
                }
            }
            RawRef::DefElem(n) => {
                self.record(n.arg_location);
                pointer(jobs, n.arg.as_deref());
            }
            RawRef::RawStmt(n) => pointer(jobs, n.stmt.as_deref()),
            RawRef::ExplainStmt(n) => pointer(jobs, n.query.as_deref()),
            RawRef::CreateRoleStmt(n) => list(jobs, &n.options),
            RawRef::AlterRoleStmt(n) => list(jobs, &n.options),
            RawRef::CreateUserMappingStmt(n) => list(jobs, &n.options),
            RawRef::AlterUserMappingStmt(n) => list(jobs, &n.options),
            RawRef::CreateSubscriptionStmt(n) if n.conninfo.is_some() => {
                self.record(n.conninfo_location)
            }
            RawRef::AlterSubscriptionStmt(n) if n.conninfo.is_some() => {
                self.record(n.conninfo_location)
            }
            RawRef::NotifyStmt(n) if n.payload.is_some() => self.record(n.payload_location),
            RawRef::TypeName(_) => {}
            RawRef::SelectStmt(_)
            | RawRef::InsertStmt(_)
            | RawRef::DeleteStmt(_)
            | RawRef::UpdateStmt(_)
            | RawRef::MergeStmt(_)
            | RawRef::VariableSetStmt(_)
            | RawRef::CopyStmt(_)
            | RawRef::CallStmt(_)
            | RawRef::DeclareCursorStmt(_)
            | RawRef::CreateFunctionStmt(_)
            | RawRef::DoStmt(_)
                if self.utility_only => {}
            RawRef::VariableSetStmt(n) => list(jobs, &n.args),
            RawRef::CopyStmt(n) => pointer(jobs, n.query.as_deref()),
            RawRef::CallStmt(n) => pointer(jobs, n.funccall.as_deref()),
            RawRef::DeclareCursorStmt(n) => pointer(jobs, n.query.as_deref()),
            RawRef::CreateFunctionStmt(n) => list(jobs, &n.options),
            RawRef::DoStmt(n) => list(jobs, &n.args),
            RawRef::SelectStmt(n) => {
                self.selects.push(Targets::new());
                select(n, jobs);
            }
            other => other.children(&mut |child| jobs.push(Job::Visit(child))),
        }
    }

    fn walk(&mut self, statements: &[RawStmt]) {
        let mut jobs: Vec<_> = statements
            .iter()
            .rev()
            .map(|n| Job::Visit(n.into()))
            .collect();
        while let Some(job) = jobs.pop() {
            let base = jobs.len();
            match job {
                Job::Visit(n) => self.visit(n, &mut jobs),
                Job::StartTarget(n) => {
                    // A nested SELECT starts its own parameter capture, even inside another target.
                    self.capture = Some(Vec::new());
                    jobs.push(Job::Visit(n.into()));
                }
                Job::EndTarget(n) => {
                    let ids = self.capture.take().unwrap_or_default();
                    if let Some(n) = n {
                        self.selects
                            .last_mut()
                            .unwrap()
                            .entry(fingerprint_node(n))
                            .or_default()
                            .push_back(ids);
                    }
                }
                Job::Group(n) => {
                    jobs.push(Job::Visit(n.into()));
                    if let Some(ids) = self
                        .selects
                        .last_mut()
                        .unwrap()
                        .get_mut(&fingerprint_node(n))
                        .and_then(VecDeque::pop_front)
                    {
                        jobs.push(Job::Reuse {
                            start: self.locations.len(),
                            ids,
                        });
                    }
                }
                Job::Reuse { start, ids } => {
                    let added = &mut self.locations[start..];
                    if added.len() == ids.len() {
                        self.assigned -= ids.len() as i64;
                        for ((_, id), target) in added.iter_mut().zip(ids) {
                            *id = target;
                        }
                    }
                }
                Job::EndSelect => {
                    self.selects.pop();
                }
            }
            jobs[base..].reverse();
        }
    }

    // [spec:pgq:req:normalize.literal-spans]
    fn render(mut self, sql: &str) -> Result<String, Error> {
        self.locations.sort_by_key(|(offset, _)| *offset);
        let mut tokens = Scanner::new(sql).filter(|r| !matches!(r, Ok(t) if t.kind.is_comment()));
        let mut saved = 0;
        let mut result = String::with_capacity(sql.len());
        for (offset, id) in self.locations {
            if offset < saved || offset >= sql.len() || !sql.is_char_boundary(offset) {
                continue;
            }
            let Some(first) = tokens
                .by_ref()
                .find(|r| !matches!(r, Ok(t) if t.start < offset))
                .transpose()
                .map_err(Error::Diagnostic)?
            else {
                break;
            };
            let end = if sql.as_bytes()[offset] == b'-' {
                let Some(next) = tokens.next().transpose().map_err(Error::Diagnostic)? else {
                    break;
                };
                next.end
            } else {
                first.end
            };
            result.push_str(&sql[saved..offset]);
            let number = if id < 0 { self.external - id } else { id };
            write!(result, "${number}").unwrap();
            saved = end;
        }
        result.push_str(&sql[saved..]);
        Ok(result)
    }
}

fn normalize_sql(sql: &str, utility_only: bool) -> Result<String, Error> {
    let sql = sql.split('\0').next().unwrap_or_default();
    let statements = parse_statements(sql, Options::default())?;
    let mut state = State {
        utility_only,
        ..State::default()
    };
    state.walk(&statements);
    state.render(sql)
}

// [spec:pgq:req:normalize.substitutions]
pub fn normalize(sql: &str) -> Result<String, Error> {
    normalize_sql(sql, false)
}

pub fn normalize_utility(sql: &str) -> Result<String, Error> {
    normalize_sql(sql, true)
}

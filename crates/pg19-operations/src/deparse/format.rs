// postgres_deparse.c groups clauses into parts. A clause with a single part
// keeps its keyword on the same line; nested queries start a fresh level.
use super::{Comment, Error, Result};

/// Formatting defaults match libpg_query's PostgresDeparseOpts. A zero indent
/// or line length selects its default, as it does in the C interface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Options {
    pub pretty_print: bool,
    pub indent_size: usize,
    pub max_line_length: usize,
    pub commas_start_of_line: bool,
    pub trailing_newline: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            pretty_print: false,
            indent_size: 4,
            max_line_length: 80,
            commas_start_of_line: false,
            trailing_newline: false,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Indent,
    Merge,
}
struct Part {
    data: String,
    indent: usize,
    merge: bool,
}
impl Part {
    fn new(indent: usize, merge: bool) -> Self {
        Self {
            data: String::new(),
            indent,
            merge,
        }
    }
}
struct Group {
    keyword: &'static str,
    mode: Mode,
    parts: Vec<Part>,
    indent: usize,
}
impl Group {
    fn new(keyword: &'static str, mode: Mode, indent: usize) -> Self {
        Self {
            keyword,
            mode,
            indent,
            parts: vec![Part::new(indent, mode == Mode::Merge)],
        }
    }
    fn part(&mut self) -> &mut Part {
        self.parts
            .last_mut()
            .expect("every group has a current part")
    }
    fn cut(&mut self, dedup: bool) {
        if dedup && self.parts.last().is_some_and(|p| p.data.is_empty()) {
            self.parts.pop();
        }
        self.parts
            .push(Part::new(self.indent, self.mode == Mode::Merge));
    }
    fn finish(self, base: usize, width: usize) -> Vec<Part> {
        let mut lines: Vec<Part> = Vec::new();
        for mut part in self.parts {
            part.data.truncate(part.data.trim_end_matches(' ').len());
            if let Some(prev) = lines.last_mut() {
                if self.mode == Mode::Merge
                    && prev.merge
                    && part.merge
                    && prev
                        .indent
                        .saturating_add(prev.data.len())
                        .saturating_add(1)
                        .saturating_add(part.data.len())
                        <= width
                {
                    if !prev.data.is_empty() && !prev.data.ends_with('(') {
                        prev.data.push(' ');
                    }
                    prev.data.push_str(&part.data);
                    continue;
                }
            }
            lines.push(part);
        }
        if !self.keyword.is_empty() {
            if lines.len() == 1 || self.mode == Mode::Flat {
                let first = &mut lines[0];
                first.indent = base;
                if !first.data.is_empty() {
                    first.data.insert(0, ' ');
                }
                first.data.insert_str(0, self.keyword);
                first.merge = false;
            } else {
                lines.insert(
                    0,
                    Part {
                        data: self.keyword.to_owned(),
                        indent: base,
                        merge: false,
                    },
                );
            }
        }
        lines
    }
}
struct Level {
    base: usize,
    groups: Vec<Group>,
}
impl Level {
    fn new(base: usize) -> Self {
        Self {
            base,
            groups: Vec::new(),
        }
    }
    fn group(&mut self) -> &mut Group {
        if self.groups.is_empty() {
            self.groups.push(Group::new("", Mode::Flat, self.base));
        }
        self.groups.last_mut().expect("initialized level")
    }
    fn close(self, width: usize) -> Vec<Part> {
        self.groups
            .into_iter()
            .flat_map(|g| g.finish(self.base, width))
            .collect()
    }
}
pub(super) struct Writer {
    options: Options,
    compact: String,
    levels: Vec<Level>,
}
impl Writer {
    pub fn new(mut options: Options) -> Self {
        if options.indent_size == 0 {
            options.indent_size = 4;
        }
        if options.max_line_length == 0 {
            options.max_line_length = 80;
        }
        Self {
            options,
            compact: String::new(),
            levels: vec![Level::new(0)],
        }
    }
    fn level(&mut self) -> &mut Level {
        self.levels.last_mut().expect("writer has a root level")
    }
    pub fn text(&mut self, s: &str) {
        if self.options.pretty_print {
            let part = self.level().group().part();
            let s = if part.data.is_empty() {
                s.trim_start_matches(' ')
            } else {
                s
            };
            part.data.push_str(s);
        } else {
            self.compact.push_str(s);
        }
    }
    pub fn clause(&mut self, keyword: &'static str) {
        if self.options.pretty_print {
            let keyword = keyword.trim_end();
            let mode = match keyword {
                "SELECT" | "GROUP BY" | "ORDER BY" | "VALUES" | "RETURNING" => Mode::Merge,
                "WITH" | "WITH RECURSIVE" | "UNION" | "EXCEPT" | "INTERSECT" => Mode::Flat,
                _ => Mode::Indent,
            };
            let indent = self.options.indent_size;
            let level = self.level();
            let base = level.base;
            level.groups.push(Group::new(
                keyword,
                mode,
                if mode == Mode::Flat {
                    base
                } else {
                    base.saturating_add(indent)
                },
            ));
        } else {
            if !self.compact.is_empty() && !self.compact.ends_with([' ', '(', '\n']) {
                self.compact.push(' ');
            }
            self.compact.push_str(keyword);
        }
    }
    pub fn cut(&mut self) {
        if self.options.pretty_print {
            self.level().group().cut(true);
        } else if !self.compact.ends_with([' ', '(']) {
            self.compact.push(' ');
        }
    }
    pub fn comma(&mut self) {
        if self.options.pretty_print {
            if self.options.commas_start_of_line {
                self.cut();
                self.text(", ");
            } else {
                self.text(",");
                self.cut();
            }
        } else {
            self.compact.push_str(", ");
        }
    }
    pub fn separate(&mut self) {
        if self.options.pretty_print {
            self.level().group().part().merge = false;
        }
    }
    pub fn enter(&mut self) -> Result<()> {
        if self.options.pretty_print {
            let size = self.options.indent_size;
            let parent = self.level().group();
            let base = parent
                .indent
                .checked_add(size)
                .ok_or(Error::InvalidField("indent width"))?;
            parent.part().merge = false;
            self.levels.push(Level::new(base));
        }
        Ok(())
    }
    pub fn leave(&mut self) {
        if self.options.pretty_print {
            let width = self.options.max_line_length;
            let nested = self.levels.pop().expect("matching enter and leave");
            let group = self.level().group();
            group.parts.extend(nested.close(width));
            group.cut(true);
            group.part().merge = false;
        }
    }
    fn line(&mut self) {
        if self.options.pretty_print {
            self.level().group().cut(false);
        } else {
            self.compact.push('\n');
        }
    }
    pub fn comment(&mut self, c: &Comment<'_>) {
        let mut before = c.newlines_before;
        if self.options.pretty_print {
            let group = self.level().group();
            if !group.keyword.is_empty() && group.part().data.is_empty() {
                before = before.saturating_sub(1);
            }
        }
        for _ in 0..before {
            self.line();
        }
        self.text(c.text);
        self.separate();
        for _ in 0..c.newlines_after {
            self.line();
        }
    }
    pub fn outer_comments(&mut self, comments: &[Comment<'_>], leading: bool) {
        if comments.is_empty() {
            return;
        }
        if self.options.pretty_print {
            self.level().groups.push(Group::new("", Mode::Flat, 0));
        } else if !leading && !self.compact.ends_with(char::is_whitespace) {
            self.compact.push(' ');
        }
        for c in comments {
            self.comment(c);
        }
        if self.options.pretty_print {
            let group = self.level().group();
            if group.parts.last().is_some_and(|part| part.data.is_empty()) {
                group.parts.pop();
            }
        } else if leading {
            self.compact.push(' ');
        }
    }
    pub fn finish(mut self) -> Result<String> {
        if !self.options.pretty_print {
            return Ok(self.compact);
        }
        let mut lines = self
            .levels
            .pop()
            .expect("balanced tree")
            .close(self.options.max_line_length);
        if lines.last().is_some_and(|p| p.data.is_empty()) {
            lines.pop();
        }
        let mut sql = String::new();
        for (i, part) in lines.into_iter().enumerate() {
            if i > 0 {
                sql.push('\n');
            }
            sql.try_reserve(part.indent)
                .map_err(|_| Error::InvalidField("indent width"))?;
            sql.extend(std::iter::repeat_n(' ', part.indent));
            sql.push_str(&part.data);
        }
        if self.options.trailing_newline {
            sql.push('\n');
        }
        Ok(sql)
    }
}

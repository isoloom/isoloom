//! A rendered Ruby file (a Vagrantfile), read as its nested `do … end` blocks.

/// A block of generated Ruby (a Vagrantfile): its opening line, its own statements and the
/// blocks inside it, read by structure. A here-document's lines belong to the statement that
/// opens it, so a script's text never reads as Ruby.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RubyBlock {
    /// The opening line, trimmed (`config.vm.define "web" do |m|`); empty for the file.
    pub head: String,
    /// The statements directly in the block, in order: each line trimmed, a here-document's
    /// lines after its opening line, joined by line breaks.
    pub statements: Vec<String>,
    /// The blocks directly inside it, in order.
    pub blocks: Vec<RubyBlock>,
}

impl RubyBlock {
    /// The blocks anywhere inside this one whose opening line starts with `head`, outermost
    /// first.
    #[must_use]
    pub fn find(&self, head: &str) -> Vec<&RubyBlock> {
        let mut out = Vec::new();
        for b in &self.blocks {
            if b.head.starts_with(head) {
                out.push(b);
            }
            out.extend(b.find(head));
        }
        out
    }

    /// The one block anywhere inside this one whose opening line starts with `head`.
    ///
    /// # Panics
    /// When there isn't exactly one.
    #[must_use]
    pub fn block(&self, head: &str) -> &RubyBlock {
        match self.find(head).as_slice() {
            [one] => one,
            found => panic!("{} blocks open with `{head}` in `{}`", found.len(), self.head),
        }
    }

    /// The statements directly in this block that start with `prefix`.
    #[must_use]
    pub fn statements(&self, prefix: &str) -> Vec<&str> {
        self.statements.iter().map(String::as_str).filter(|s| s.starts_with(prefix)).collect()
    }

    /// Every statement in this block and the blocks inside it, the opening lines included: what
    /// a test asks of a whole block ("does the VM's define set this anywhere?").
    #[must_use]
    pub fn every_statement(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self.statements.iter().map(String::as_str).collect();
        for b in &self.blocks {
            out.push(&b.head);
            out.extend(b.every_statement());
        }
        out
    }
}

/// A generated Ruby file (a Vagrantfile), read into its blocks: `do … end`, and `if`, `unless`,
/// `case`, `def`, `class`, `module`, `begin`, `while` and `until` opening a line, each closed by
/// its `end`. A string literal over several lines, and a here-document, belong to the statement
/// that opens them.
///
/// # Panics
/// When a block isn't closed, or an `end` closes none.
#[must_use]
pub fn ruby_blocks(text: &str) -> RubyBlock {
    let mut stack = vec![RubyBlock::default()];
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "end" || trimmed.starts_with("end.") || trimmed.starts_with("end ") || trimmed.starts_with("end)") {
            let done = stack.pop().unwrap_or_else(|| panic!("`end` closes no block:\n{text}"));
            let Some(parent) = stack.last_mut() else {
                panic!("`end` closes no block:\n{text}")
            };
            parent.blocks.push(done);
            continue;
        }
        let mut statement = trimmed.to_owned();
        let (mut open, mut tag) = scan(trimmed, None);
        while open.is_some() {
            let Some(next) = lines.next() else { break };
            statement.push('\n');
            statement.push_str(next.trim());
            let (still, more) = scan(next, open);
            open = still;
            tag = tag.or(more);
        }
        if let Some(tag) = tag {
            for body in lines.by_ref() {
                statement.push('\n');
                statement.push_str(body.trim());
                if body.trim() == tag {
                    break;
                }
            }
        }
        if opens_block(trimmed) {
            stack.push(RubyBlock {
                head: statement,
                ..RubyBlock::default()
            });
        } else if let Some(block) = stack.last_mut() {
            block.statements.push(statement);
        }
    }
    match stack.len() {
        1 => stack.pop().unwrap_or_default(),
        n => panic!("{} block(s) never closed:\n{text}", n - 1),
    }
}

/// Reads `line` from inside the string literal opened by `quote` (none: from code): the quote
/// still open at its end, and the terminator of the first here-document it opens outside a
/// string (`<<~'SH'`, `<<-EOS`, `<<RUBY`). A comment ends the code.
fn scan(line: &str, mut quote: Option<char>) -> (Option<char>, Option<String>) {
    let mut tag = None;
    let mut chars = line.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        match quote {
            Some(_) if c == '\\' => {
                chars.next();
            }
            Some(q) if c == q => quote = None,
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => break,
            None if tag.is_none() && line.get(at..).is_some_and(|r| r.starts_with("<<")) => {
                let after = line.get(at + 2..).unwrap_or_default().trim_start_matches(['~', '-']);
                let quoted = after.starts_with(['\'', '"']);
                let name: String = after
                    .trim_start_matches(['\'', '"'])
                    .chars()
                    .take_while(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || *ch == '_')
                    .collect();
                if name.chars().next().is_some_and(|ch| ch.is_ascii_uppercase()) {
                    // Past the opener, its quoted tag included, so its quotes open no string.
                    let skip = (line.len() - at - after.len()) + name.len() + if quoted { 2 } else { 0 } - 1;
                    for _ in 0..skip {
                        chars.next();
                    }
                    tag = Some(name);
                }
            }
            _ => {}
        }
    }
    (quote, tag)
}

/// Whether `line` (trimmed, a comment's text left out) opens a block its own `end` closes.
fn opens_block(line: &str) -> bool {
    let code = line.split(" # ").next().unwrap_or(line).trim_end();
    let keyword = ["if ", "unless ", "case ", "def ", "class ", "module ", "while ", "until "]
        .iter()
        .any(|k| code.starts_with(k))
        || code == "begin"
        || code.ends_with(" begin");
    let do_block = code.ends_with(" do") || (code.contains(" do |") && code.ends_with('|'));
    (keyword || do_block) && !code.ends_with(" end") && !code.contains("; end")
}

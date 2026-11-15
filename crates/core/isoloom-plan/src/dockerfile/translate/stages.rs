//! A Dockerfile's stages: where each starts, and which the last one needs.

use super::super::instruction::{Copy, Instruction, Line, Source};

/// The stages of `lines`: each its lines (the first's with the global ARGs before its FROM) and
/// its name.
pub(super) fn split_stages(lines: &[Line]) -> Vec<(Vec<Line>, Option<String>)> {
    let mut out: Vec<(Vec<Line>, Option<String>)> = Vec::new();
    let mut pending: Vec<Line> = Vec::new();
    for line in lines {
        if let Instruction::From { stage, .. } = &line.instruction {
            if out.is_empty() {
                pending.push(line.clone());
                out.push((std::mem::take(&mut pending), stage.clone()));
            } else {
                out.push((vec![line.clone()], stage.clone()));
            }
            continue;
        }
        match out.last_mut() {
            Some((stage, _)) => stage.push(line.clone()),
            None => pending.push(line.clone()),
        }
    }
    out
}

/// The index of the stage named `name` (or numbered).
pub(super) fn stage_index(stages: &[(Vec<Line>, Option<String>)], name: &str) -> Option<usize> {
    let name = name.to_ascii_lowercase();
    stages.iter().position(|(_, n)| n.as_deref() == Some(name.as_str()))
}

/// The stages `last` needs: its `FROM` chain and the stages it copies from, transitively.
pub(super) fn needed(stages: &[(Vec<Line>, Option<String>)], last: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut todo = vec![last];
    while let Some(i) = todo.pop() {
        if out.contains(&i) {
            continue;
        }
        out.push(i);
        let Some((lines, _)) = stages.get(i) else { continue };
        for line in lines {
            let named = match &line.instruction {
                Instruction::From { image, .. } => Some(image.clone()),
                Instruction::Copy(Copy { source: Source::From(s), .. }) | Instruction::Add(Copy { source: Source::From(s), .. }) => Some(s.clone()),
                _ => None,
            };
            if let Some(j) = named.and_then(|n| stage_index(stages, &n).or_else(|| n.parse().ok())) {
                todo.push(j);
            }
        }
    }
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::super::super::instruction;
    use super::*;

    #[test]
    fn only_the_last_stage_and_what_it_needs_run() {
        let lines = instruction::read("FROM debian:12 AS a\nRUN a\nFROM debian:12 AS unused\nRUN b\nFROM a AS c\nCOPY --from=0 /x /y\n").unwrap();
        let stages = split_stages(&lines);
        assert_eq!(needed(&stages, 2), [0, 2]);
    }
}

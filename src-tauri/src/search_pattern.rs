use crate::search::Plan;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};

pub fn matcher(plan: &Plan) -> Result<RegexMatcher, String> {
    let fixed = plan.flags.contains(&"-F");
    let pattern = if fixed {
        plan.pattern.clone()
    } else {
        basic(&plan.pattern)
    };
    RegexMatcherBuilder::new()
        .unicode(true)
        .multi_line(true)
        .case_insensitive(plan.flags.contains(&"-i"))
        .word(plan.flags.contains(&"-w"))
        .fixed_strings(fixed)
        .build(&pattern)
        .map_err(|error| format!("Invalid search expression: {error}"))
}

pub fn needs_git(plan: &Plan) -> bool {
    if !plan.flags.contains(&"-G") {
        return false;
    }
    let mut chars = plan.pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.next().is_some_and(|next| next.is_ascii_digit()) {
            return true;
        }
    }
    false
}

fn basic(pattern: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' if !in_class => match chars.next() {
                Some(next @ ('(' | ')' | '{' | '}' | '+' | '?' | '|')) => out.push(next),
                Some('<') => out.push_str("\\b{start}"),
                Some('>') => out.push_str("\\b{end}"),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            },
            '[' => {
                in_class = true;
                out.push(c);
            }
            ']' => {
                in_class = false;
                out.push(c);
            }
            '(' | ')' | '{' | '}' | '+' | '?' | '|' if !in_class => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

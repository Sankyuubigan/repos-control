pub const SYSTEM_PROMPT: &str = "Ты — полезный ассистент, который генерирует информативные git commit-сообщения на основе вывода git diff. Пропусти преамбулу и убери все обратные кавычки вокруг commit-сообщения.";

pub const INSTRUCTION_PROMPT: &str = "На основе предоставленного git diff сгенерируй краткое и ёмкое commit-сообщение. Руководствуйся следующими правилами:
1. Начни с короткого заголовка — не более 50-72 символов.
2. Используй conventional commits (feat:, fix:, refactor:, chore:, docs:, test:, perf:).
3. Опиши, что изменилось и почему.
4. Пиши ясно и информативно.";

pub const MAX_DIFF_CHARS: usize = 5000;
pub const TRUNCATION_MARKER: &str = "[Diff truncated due to size]";

pub fn build_prompt(notes: &str, diff: &str) -> String {
    let truncated = truncate_diff(diff);
    let user = format!(
        "Заметки разработчика (игнорируй, если нерелевантно): {notes}\n\nИзменения:\n{truncated}"
    );
    format!("{SYSTEM_PROMPT}\n\n{INSTRUCTION_PROMPT}\n\n{user}")
}

fn truncate_diff(diff: &str) -> String {
    if diff.len() <= MAX_DIFF_CHARS {
        return diff.to_string();
    }
    format!("{}{TRUNCATION_MARKER}", &diff[..MAX_DIFF_CHARS])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_long_diff() {
        let big = "x".repeat(MAX_DIFF_CHARS + 10);
        let out = truncate_diff(&big);
        assert!(out.ends_with(TRUNCATION_MARKER));
        assert!(out.starts_with(&"x".repeat(MAX_DIFF_CHARS)));
    }

    #[test]
    fn short_diff_untouched() {
        let s = "feat: ok";
        assert_eq!(truncate_diff(s), s);
    }
}
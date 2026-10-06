use serde::{Deserialize, Serialize};

use crate::domain::contracts::DiffScope;

pub const SYSTEM_PROMPT: &str = "Ты — полезный ассистент, который генерирует информативные git commit-сообщения на основе вывода git diff. Пропусти преамбулу и убери все обратные кавычки вокруг commit-сообщения.";

pub const INSTRUCTION_PROMPT: &str = "На основе предоставленного git diff сгенерируй краткое и ёмкое commit-сообщение. Руководствуйся следующими правилами:
1. Начни с короткого заголовка — не более 50-72 символов.
2. Используй conventional commits (feat:, fix:, refactor:, chore:, docs:, test:, perf:).
3. Опиши, что изменилось и почему.
4. Пиши ясно и информативно.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub fn build_messages(notes: &str, diff: &str, lang: &str, scope: DiffScope) -> Vec<ChatMessage> {
    let scope_label = match scope {
        DiffScope::Staged => "стейдж индекс",
        DiffScope::Unstaged => "остальные изменения",
        DiffScope::All => "все незакоммиченные изменения",
    };
    let user = format!(
        "Заметки разработчика (игнорируй, если нерелевантно): {notes}\n\nИзменения ({scope_label}):\n{diff}\n\nОтветь на языке: {lang}."
    );
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: format!("{SYSTEM_PROMPT}\n\n{INSTRUCTION_PROMPT}"),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_is_not_truncated() {
        let diff = "юникод ★".repeat(2000);
        assert!(diff.len() > 5000, "фикстура должна быть длиннее прежнего лимита");
        let messages = build_messages("", &diff, "ru", DiffScope::All);
        assert!(messages[1].content.contains("★"));
        assert!(!messages[1].content.contains("truncated"));
    }

    #[test]
    fn removed_files_reach_prompt() {
        let diff = "diff --git a/gone.txt b/gone.txt\ndeleted file mode 100644";
        let messages = build_messages("", diff, "ru", DiffScope::All);
        assert!(messages[1].content.contains("deleted file mode"));
    }
}
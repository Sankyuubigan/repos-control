use crate::domain::contracts::CommitMessageProvider;

pub struct StubCommitProvider;

impl CommitMessageProvider for StubCommitProvider {
    fn generate(&self, _prompt: &str) -> Result<String, anyhow::Error> {
        Ok("feat: [Заглушка LLM] сгенерируйте сообщение вручную".to_string())
    }
}
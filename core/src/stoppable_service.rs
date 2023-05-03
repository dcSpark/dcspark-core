#[async_trait::async_trait]
pub trait StoppableService {
    async fn stop(self) -> anyhow::Result<()>;
}

use obsidian_mcp::tools::ObsidianTools;
use rmcp::ServiceExt;
use rmcp::transport::stdio;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = ObsidianTools::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

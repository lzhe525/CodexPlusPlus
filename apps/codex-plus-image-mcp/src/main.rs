#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.as_slice() == ["--enterprise-credential", "get"] {
        if codex_plus_core::enterprise::current_login_method()
            != codex_plus_core::enterprise::LOGIN_METHOD_COMPANY
        {
            anyhow::bail!("enterprise credential is disabled for official account login");
        }
        let credential = codex_plus_core::enterprise::enterprise_credential()?
            .ok_or_else(|| anyhow::anyhow!("enterprise credential is unavailable"))?;
        print!("{credential}");
        return Ok(());
    }
    codex_plus_core::enterprise_image_mcp::run().await
}

use coevo_core::metadata::CommonMetadataHeader;
use coevo_mcl::compiler::MCLCompiler;

pub async fn run(intent: &str, mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    let meta = CommonMetadataHeader::new(
        "cli-compile".to_string(),
        "0".repeat(64),
        "cli-tenant".to_string(),
        "cli-plan".to_string(),
        "CLI".to_string(),
    );

    let compiler = MCLCompiler::new();
    let result = compiler.compile(intent, mode, None, &meta).await?;

    println!("Contract Hash: {}", result.contract_hash);
    println!("Ambiguity: {:.2}", result.ambiguity_score);
    println!("Warnings: {:?}", result.compile_warnings);
    println!(
        "Contract JSON:\n{}",
        serde_json::to_string_pretty(&result.contract)?
    );
    Ok(())
}

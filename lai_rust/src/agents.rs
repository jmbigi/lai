pub async fn cmd_think(problem: &str, model: &str, base_url: &str, allow_remote: bool) {
    let client = crate::llm::LlmClient::new(base_url.to_string(), model.to_string());
    match client.chat(problem).await { Ok(resp) => println!("{}", resp), Err(e) => eprintln!("Error: {}", e), } \
}

fn main() {
    let content = "```json\n{\"query\": \"rust\"}\n```";
    let clean = content.trim().strip_prefix("```json").unwrap_or(content).strip_prefix("```").unwrap_or(content).strip_suffix("```").unwrap_or(content).trim();
    println!("{}", clean);
}

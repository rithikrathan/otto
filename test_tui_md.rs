fn main() {
    let doc = "**Topic:** `rust`\n\n```\n/*\n * You want to use the [buffered reader...\n```\n";
    let _md = tui_markdown::from_str(doc);
    println!("OK");
}

pub fn two_word_preview(task: &str) -> String {
    let stop = ["the","a","an","to","for","and","of","in"];
    let words: Vec<&str> = task
        .split_whitespace()
        .filter(|w| !stop.contains(&w.to_ascii_lowercase().as_str()))
        .take(2)
        .collect();
    words.join(" ").to_ascii_lowercase()
}

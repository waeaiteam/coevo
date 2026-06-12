pub fn is_plain_identifier(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && !trimmed.contains("..")
        && !trimmed.contains('\\')
        && !trimmed.contains('/')
        && !trimmed.contains(':')
}

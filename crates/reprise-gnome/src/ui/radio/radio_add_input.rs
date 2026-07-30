#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AddInput {
    Empty,
    Search(String),
    Url(String),
}

pub(super) fn classify_input(input: &str) -> AddInput {
    let input = input.trim();
    if input.is_empty() {
        return AddInput::Empty;
    }
    if input.starts_with("http://") || input.starts_with("https://") {
        AddInput::Url(input.to_owned())
    } else {
        AddInput::Search(input.to_owned())
    }
}

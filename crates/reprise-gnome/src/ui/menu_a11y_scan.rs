//! The textual scanners behind `menu_a11y_guard_tests.rs`.
//!
//! They read Rust source as text, so each rule is a heuristic and is spelled out
//! where it is applied. The fixture tests in the guard module pin what each one
//! catches and what it leaves alone.

const POPOVER_MENU: &str = "PopoverMenu";
const MENU_BUTTON: &str = "MenuButton";

/// The production code of `source`: without comment lines and without any
/// `#[cfg(test)] mod …` (inline or `#[path]`-wired test module), wherever in the
/// file the module sits. Production code after a test module is kept. A
/// `#[cfg(test)]` on a field or function must not hide anything, so only a `mod`
/// item is skipped.
pub(super) fn production_code(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut kept = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if let Some(end) = test_module_end(&lines, index) {
            index = end;
            continue;
        }
        if !lines[index].trim_start().starts_with("//") {
            kept.push(lines[index]);
        }
        index += 1;
    }
    kept.join("\n")
}

/// When `lines[start]` opens a `#[cfg(test)] mod`, the index of the first line
/// after the module; `None` for anything else.
fn test_module_end(lines: &[&str], start: usize) -> Option<usize> {
    if lines[start].trim() != "#[cfg(test)]" {
        return None;
    }
    let item = (start + 1..lines.len()).find(|&index| {
        let line = lines[index].trim();
        !line.starts_with("#[") && !line.starts_with("//")
    })?;
    is_module_item(lines[item].trim()).then(|| item_end(lines, item))
}

fn is_module_item(line: &str) -> bool {
    let line = line
        .strip_prefix("pub")
        .map_or(line, |rest| match rest.find(')') {
            Some(close) if rest.starts_with('(') => rest[close + 1..].trim_start(),
            _ => rest.trim_start(),
        });
    line.starts_with("mod ")
}

/// Where Rust lexing stands between two characters of the module being skipped.
enum Lex {
    Code,
    Str,
    RawStr(usize),
    BlockComment(usize),
}

/// The index of the first line after the item starting at `lines[start]`: after
/// the first `;` when the item has no body (`mod tests;`), otherwise after the
/// `}` that closes its body. Braces inside strings, character literals and
/// comments do not count. An unclosed body runs to the end of the file.
fn item_end(lines: &[&str], start: usize) -> usize {
    let mut state = Lex::Code;
    let mut depth = 0usize;
    for (index, line) in lines.iter().enumerate().skip(start) {
        let chars: Vec<char> = line.chars().collect();
        let mut at = 0;
        while at < chars.len() {
            let current = chars[at];
            let next = chars.get(at + 1).copied();
            match state {
                Lex::Code => match current {
                    '/' if next == Some('/') => break,
                    '/' if next == Some('*') => {
                        state = Lex::BlockComment(1);
                        at += 1;
                    }
                    '"' => state = Lex::Str,
                    'r' if at == 0 || !is_ident(chars[at - 1]) || chars[at - 1] == 'b' => {
                        let hashes = chars[at + 1..].iter().take_while(|&&c| c == '#').count();
                        if chars.get(at + 1 + hashes) == Some(&'"') {
                            state = Lex::RawStr(hashes);
                            at += 1 + hashes;
                        }
                    }
                    '\'' => at += char_literal_extra(&chars[at..]),
                    '{' => depth += 1,
                    '}' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return index + 1;
                        }
                    }
                    ';' if depth == 0 => return index + 1,
                    _ => {}
                },
                Lex::Str => match current {
                    '\\' => at += 1,
                    '"' => state = Lex::Code,
                    _ => {}
                },
                Lex::RawStr(hashes) => {
                    let closes = current == '"'
                        && chars[at + 1..].iter().take_while(|&&c| c == '#').count() >= hashes;
                    if closes {
                        state = Lex::Code;
                        at += hashes;
                    }
                }
                Lex::BlockComment(nesting) => match (current, next) {
                    ('*', Some('/')) => {
                        state = if nesting == 1 {
                            Lex::Code
                        } else {
                            Lex::BlockComment(nesting - 1)
                        };
                        at += 1;
                    }
                    ('/', Some('*')) => {
                        state = Lex::BlockComment(nesting + 1);
                        at += 1;
                    }
                    _ => {}
                },
            }
            at += 1;
        }
    }
    lines.len()
}

/// How many characters past the opening quote a character literal at the start
/// of `rest` spans, or 0 for a lifetime (`'a`).
fn char_literal_extra(rest: &[char]) -> usize {
    match (rest.get(1), rest.get(2)) {
        (Some('\\'), _) => rest
            .get(3..)
            .and_then(|tail| tail.iter().position(|&c| c == '\''))
            .map_or(0, |close| close + 3),
        (Some(_), Some('\'')) => 2,
        _ => 0,
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The names the `PopoverMenu` type goes by in `code`: its own, every
/// `use … PopoverMenu as Alias` and every `type Alias = …PopoverMenu;`.
fn popover_menu_names(code: &str) -> Vec<String> {
    let tokens: Vec<&str> = code
        .split(|c: char| !is_ident(c))
        .filter(|token| !token.is_empty())
        .collect();
    let mut names = vec![POPOVER_MENU.to_owned()];
    for window in tokens.windows(3) {
        if window[0] == POPOVER_MENU && window[1] == "as" {
            names.push(window[2].to_owned());
        }
    }
    for (at, _) in code.match_indices("type ") {
        if at > 0 && is_ident(code[..at].chars().next_back().unwrap_or(' ')) {
            continue;
        }
        let statement = code[at + "type ".len()..]
            .split(';')
            .next()
            .unwrap_or_default();
        if let Some((alias, target)) = statement.split_once('=') {
            let last = target.rsplit("::").next().unwrap_or_default().trim();
            if last == POPOVER_MENU && !alias.contains('<') {
                names.push(alias.trim().to_owned());
            }
        }
    }
    names
}

/// A `PopoverMenu` constructor: any associated function called on the type or on
/// an alias of it, or `Object::new::<…>` / `Object::builder::<…>` instantiating
/// it. `downcast::<PopoverMenu>()` and type annotations have no `::` after the
/// name.
pub(super) fn builds_a_popover_menu(code: &str) -> bool {
    let names = popover_menu_names(code);
    calls_an_associated_function(code, &names) || instantiates_generically(code, &names)
}

fn calls_an_associated_function(code: &str, names: &[String]) -> bool {
    names.iter().any(|name| {
        let call = format!("{name}::");
        code.match_indices(&call).any(|(at, _)| {
            // The type's own name matches inside a longer path or name too; an
            // alias must stand alone, or `HTPM::new` would count as `PM::new`.
            let standalone =
                name == POPOVER_MENU || !code[..at].chars().next_back().is_some_and(is_ident);
            // `PopoverMenuBar::` and friends are other types.
            standalone && code[at + call.len()..].starts_with(|c: char| c.is_ascii_lowercase())
        })
    })
}

/// `Object::new::<T>()` and `Object::builder::<T>()` with a `T` that mentions
/// one of `names`. Whitespace is dropped first because rustfmt may break the
/// turbofish across lines.
fn instantiates_generically(code: &str, names: &[String]) -> bool {
    let compact = without_whitespace(code);
    ["Object::new::<", "Object::builder::<"]
        .iter()
        .flat_map(|opening| compact.match_indices(opening))
        .any(|(at, opening)| {
            let generic = generic_argument(&compact[at + opening.len()..]);
            generic
                .split(|c: char| !is_ident(c))
                .any(|token| names.iter().any(|name| name == token))
        })
}

/// The text up to the `>` that closes an already-opened `<`.
fn generic_argument(after_open: &str) -> &str {
    let mut depth = 1usize;
    for (at, c) in after_open.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return &after_open[..at];
                }
            }
            _ => {}
        }
    }
    after_open
}

fn without_whitespace(code: &str) -> String {
    code.chars().filter(|c| !c.is_whitespace()).collect()
}

/// How many menu models `code` gives to a `MenuButton`, each of which needs its
/// own `name_menu_button_items` call.
///
/// A site is `.menu_model(<argument>)` on a builder, or `.set_menu_model(` on a
/// receiver that is not a popover. The argument may sit on the next line;
/// `.menu_model()` with no argument is the getter. Whitespace and line breaks
/// between the tokens are ignored.
pub(super) fn menu_button_model_sites(code: &str) -> usize {
    let compact = without_whitespace(code);
    let builder = compact
        .match_indices(".menu_model(")
        .filter(|(at, name)| !compact[at + name.len()..].starts_with(')'))
        .count();
    let setter = compact
        .match_indices(".set_menu_model(")
        .filter(|(at, _)| !receiver_is_a_popover(&compact, *at))
        .count();
    builder + setter
}

/// Whether the expression ending at `dot` in `compact` is a popover.
///
/// `PopoverMenu` has its own `set_menu_model`, which needs no naming call. The
/// receiver's last identifier decides: one that mentions a popover is a popover,
/// unless it also says "button" or `compact` declares it as a `MenuButton`
/// (`let popover_trigger = MenuButton::new()`, `popover_trigger: MenuButton`).
fn receiver_is_a_popover(compact: &str, dot: usize) -> bool {
    let before = compact[..dot].trim_end_matches(')');
    let start = before.rfind(|c: char| !is_ident(c)).map_or(0, |at| at + 1);
    let name = &before[start..];
    let lower = name.to_ascii_lowercase();
    lower.contains("popover")
        && !lower.contains("button")
        && !declared_as_menu_button(compact, name)
}

/// Whether `compact` declares `name` with a type or initialiser that mentions
/// `MenuButton`: a `let` binding, a field or a parameter.
fn declared_as_menu_button(compact: &str, name: &str) -> bool {
    let binding = ["let", "letmut"].iter().any(|keyword| {
        let declaration = format!("{keyword}{name}");
        compact.match_indices(&declaration).any(|(at, _)| {
            let rest = &compact[at + declaration.len()..];
            rest.starts_with(['=', ':'])
                && rest
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .contains(MENU_BUTTON)
        })
    });
    let field = format!("{name}:");
    binding
        || compact.match_indices(&field).any(|(at, _)| {
            let standalone = !compact[..at].chars().next_back().is_some_and(is_ident);
            let declared_type = compact[at + field.len()..]
                .split([',', ';', ')', '=', '{', '}'])
                .next()
                .unwrap_or_default();
            standalone && declared_type.contains(MENU_BUTTON)
        })
}

pub(super) fn named_menu_button_calls(code: &str) -> usize {
    code.matches("menu_a11y::name_menu_button_items(").count()
}

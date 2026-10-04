//! Light tidying of a transcript before it is pasted: whitespace collapsed,
//! a capital letter at the start of every sentence and a punctuation mark at
//! the end. Capitals are only ever added, never removed.

/// Abbreviations whose period doesn't end a sentence (lowercase, without the
/// period). Abbreviations with an inner period (т.е., e.g.) are caught by
/// that period anyway.
const ABBREVIATIONS: &[&str] = &[
    "напр", "см", "ср", "др", "пр", "стр", "рис", "табл", "им", "ул", "пр-т", "e.g", "i.e", "vs",
    "mr", "mrs", "ms", "dr", "st", "etc",
];

/// Closing quotes and brackets that may follow a sentence's last mark.
fn is_closer(ch: char) -> bool {
    matches!(ch, '"' | '\'' | '»' | '”' | '’' | ')' | ']' | '}')
}

/// Opening quotes and brackets that may come before a sentence's first letter.
fn is_opener(ch: char) -> bool {
    matches!(ch, '"' | '\'' | '«' | '“' | '‘' | '(' | '[' | '{' | '-' | '—' | '–')
}

/// Whether the period that ends `word` (the text since the last space,
/// period included) closes a sentence. Not for decimals and addresses (no
/// space follows those anyway) nor for abbreviations.
fn period_ends_sentence(word: &str) -> bool {
    let body = word.trim_end_matches('.').trim_start_matches(is_opener);
    if body.contains('.') {
        // т.е. / e.g. / и т.д.: an abbreviation with inner periods
        return false;
    }
    !ABBREVIATIONS.contains(&body.to_lowercase().as_str())
}

pub fn cleanup_text(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(text.len() + 1);
    let mut capitalize_next = true;
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            result.push(' ');
        }
        // Words with a capital already (iPhone, eBay, USA) stay as they are
        let mut capitalized = !capitalize_next || word.chars().any(char::is_uppercase);
        for ch in word.chars() {
            if !capitalized && ch.is_alphabetic() {
                result.extend(ch.to_uppercase());
                capitalized = true;
            } else {
                if !capitalized && !is_opener(ch) {
                    // A sentence starting with a number or a symbol keeps it as is
                    capitalized = true;
                }
                result.push(ch);
            }
        }

        // Does this word end a sentence? Only . ! ? count (an ellipsis
        // usually goes on in lowercase), possibly followed by closers.
        let end = word.trim_end_matches(is_closer);
        capitalize_next = if end.ends_with("...") || end.ends_with('…') {
            false
        } else if end.ends_with('!') || end.ends_with('?') {
            true
        } else if end.ends_with('.') {
            period_ends_sentence(end)
        } else {
            false
        };
    }

    // Ensure ending punctuation
    let end = result.trim_end_matches(is_closer);
    match end.chars().last() {
        Some('.' | '!' | '?' | '…' | ':') => {}
        Some(',' | ';') => {
            // A trailing comma becomes the period of the sentence
            let cut = end.len() - 1;
            let closers = result[end.len()..].to_string();
            result.truncate(cut);
            result.push('.');
            result.push_str(&closers);
        }
        _ => result.push('.'),
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_whitespace() {
        assert_eq!(cleanup_text("  hello world  "), "Hello world.");
    }

    #[test]
    fn test_normalize_spaces() {
        assert_eq!(cleanup_text("hello    world"), "Hello world.");
        assert_eq!(cleanup_text("hello\n\tworld"), "Hello world.");
    }

    #[test]
    fn test_capitalize_after_sentence_end() {
        assert_eq!(cleanup_text("hello. world"), "Hello. World.");
        assert_eq!(cleanup_text("hello? world"), "Hello? World.");
        assert_eq!(cleanup_text("hello! world"), "Hello! World.");
        assert_eq!(cleanup_text("привет. как дела"), "Привет. Как дела.");
        assert_eq!(cleanup_text("он сказал «да.» потом ушёл"), "Он сказал «да.» Потом ушёл.");
    }

    #[test]
    fn test_no_capitals_inside_numbers_addresses_and_abbreviations() {
        assert_eq!(cleanup_text("версия 3.14 вышла"), "Версия 3.14 вышла.");
        assert_eq!(cleanup_text("open example.com now"), "Open example.com now.");
        assert_eq!(cleanup_text("это т.е. вот так"), "Это т.е. вот так.");
        assert_eq!(cleanup_text("яблоки, груши и т.д. и т.п."), "Яблоки, груши и т.д. и т.п.");
        assert_eq!(cleanup_text("см. рисунок ниже"), "См. рисунок ниже.");
        assert_eq!(cleanup_text("use tools e.g. hammers"), "Use tools e.g. hammers.");
    }

    #[test]
    fn test_ellipsis_keeps_lowercase() {
        assert_eq!(cleanup_text("ну... ладно"), "Ну... ладно.");
        assert_eq!(cleanup_text("ну… ладно"), "Ну… ладно.");
    }

    #[test]
    fn test_sentence_starting_with_a_number_or_quote() {
        assert_eq!(cleanup_text("5 apples. «ok» then"), "5 apples. «Ok» then.");
        assert_eq!(cleanup_text("iPhone is here"), "iPhone is here.");
    }

    #[test]
    fn test_ending_punctuation() {
        assert_eq!(cleanup_text("hello world"), "Hello world.");
        assert_eq!(cleanup_text("hello world."), "Hello world.");
        assert_eq!(cleanup_text("hello world!"), "Hello world!");
        assert_eq!(cleanup_text("hello world?"), "Hello world?");
        assert_eq!(cleanup_text("и так далее…"), "И так далее…");
        assert_eq!(cleanup_text("he said \"stop!\""), "He said \"stop!\"");
        assert_eq!(cleanup_text("(как-то так)"), "(Как-то так).");
        assert_eq!(cleanup_text("список:"), "Список:");
        assert_eq!(cleanup_text("hello world,"), "Hello world.");
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(cleanup_text(""), "");
        assert_eq!(cleanup_text("   "), "");
    }

    #[test]
    fn test_already_clean() {
        assert_eq!(cleanup_text("Hello world."), "Hello world.");
    }
}

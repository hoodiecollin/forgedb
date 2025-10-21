use forgedb_fulltext::*;

#[test]
fn test_tokenize_simple_words() {
    let tokens = Tokenizer::tokenize("hello world");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
}

#[test]
fn test_tokenize_empty_string() {
    let tokens = Tokenizer::tokenize("");
    assert_eq!(tokens.len(), 0);
}

#[test]
fn test_tokenize_whitespace_only() {
    let tokens = Tokenizer::tokenize("   \t\n  ");
    assert_eq!(tokens.len(), 0);
}

#[test]
fn test_tokenize_single_word() {
    let tokens = Tokenizer::tokenize("hello");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[0].position, 0);
}

#[test]
fn test_tokenize_with_punctuation() {
    let tokens = Tokenizer::tokenize("Hello, world!");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
}

#[test]
fn test_tokenize_multiple_punctuation() {
    let tokens = Tokenizer::tokenize("Hello!!! World??? Test...");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
    assert_eq!(tokens[2].text, "test");
}

#[test]
fn test_tokenize_positions() {
    let tokens = Tokenizer::tokenize("one two three four");
    assert_eq!(tokens.len(), 4);
    assert_eq!(tokens[0].position, 0);
    assert_eq!(tokens[1].position, 1);
    assert_eq!(tokens[2].position, 2);
    assert_eq!(tokens[3].position, 3);
}

#[test]
fn test_tokenize_case_normalization() {
    let tokens = Tokenizer::tokenize("HELLO World TeSt");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
    assert_eq!(tokens[2].text, "test");
}

#[test]
fn test_tokenize_with_numbers() {
    let tokens = Tokenizer::tokenize("version 1 2 3");
    assert_eq!(tokens.len(), 4);
    assert_eq!(tokens[0].text, "version");
    assert_eq!(tokens[1].text, "1");
    assert_eq!(tokens[2].text, "2");
    assert_eq!(tokens[3].text, "3");
}

#[test]
fn test_tokenize_hyphenated_words() {
    let tokens = Tokenizer::tokenize("state-of-the-art");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].text, "state-of-the-art");
}

#[test]
fn test_tokenize_underscores() {
    let tokens = Tokenizer::tokenize("my_variable_name");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].text, "my_variable_name");
}

#[test]
fn test_tokenize_mixed_separators() {
    let tokens = Tokenizer::tokenize("test-name_here");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].text, "test-name_here");
}

#[test]
fn test_tokenize_removes_leading_punctuation() {
    let tokens = Tokenizer::tokenize("!!!hello ..world");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
}

#[test]
fn test_tokenize_removes_trailing_punctuation() {
    let tokens = Tokenizer::tokenize("hello!!! world...");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
}

#[test]
fn test_tokenize_multiple_spaces() {
    let tokens = Tokenizer::tokenize("hello    world     test");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
    assert_eq!(tokens[2].text, "test");
}

#[test]
fn test_tokenize_tabs_and_newlines() {
    let tokens = Tokenizer::tokenize("hello\tworld\ntest");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
    assert_eq!(tokens[2].text, "test");
}

#[test]
fn test_tokenize_unicode_text() {
    let tokens = Tokenizer::tokenize("hello 世界");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "世界");
}

#[test]
fn test_tokenize_emoji() {
    let tokens = Tokenizer::tokenize("hello 😀 world");
    // Emojis are removed as they're not alphanumeric, hyphen, or underscore
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].text, "hello");
    assert_eq!(tokens[1].text, "world");
}

#[test]
fn test_tokenize_special_characters() {
    let tokens = Tokenizer::tokenize("test@email.com #hashtag $money");
    // Special chars are removed, resulting in concatenated alphanumeric strings
    assert!(tokens.len() >= 3);
    assert!(tokens.iter().any(|t| t.text.contains("test")));
    assert!(tokens.iter().any(|t| t.text.contains("hashtag")));
    assert!(tokens.iter().any(|t| t.text.contains("money")));
}

#[test]
fn test_trigrams_basic() {
    let trigrams = Tokenizer::trigrams("hello");
    assert_eq!(trigrams, vec!["hel", "ell", "llo"]);
}

#[test]
fn test_trigrams_short_word() {
    let trigrams = Tokenizer::trigrams("hi");
    assert_eq!(trigrams, vec!["hi"]);
}

#[test]
fn test_trigrams_single_char() {
    let trigrams = Tokenizer::trigrams("a");
    assert_eq!(trigrams, vec!["a"]);
}

#[test]
fn test_trigrams_empty_string() {
    let trigrams = Tokenizer::trigrams("");
    assert_eq!(trigrams, vec![""]);
}

#[test]
fn test_trigrams_three_chars() {
    let trigrams = Tokenizer::trigrams("abc");
    assert_eq!(trigrams, vec!["abc"]);
}

#[test]
fn test_trigrams_four_chars() {
    let trigrams = Tokenizer::trigrams("abcd");
    assert_eq!(trigrams, vec!["abc", "bcd"]);
}

#[test]
fn test_trigrams_longer_word() {
    let trigrams = Tokenizer::trigrams("programming");
    assert_eq!(trigrams.len(), 9); // "programming" has 11 chars, so 11-3+1 = 9 trigrams
    assert_eq!(trigrams[0], "pro");
    assert_eq!(trigrams[1], "rog");
    assert!(trigrams.contains(&"ram".to_string()));
}

#[test]
fn test_trigrams_unicode() {
    let trigrams = Tokenizer::trigrams("世界和平");
    // Should handle unicode characters
    assert!(trigrams.len() >= 2);
}

#[test]
fn test_tokenize_preserves_alphanumeric_with_hyphens() {
    let tokens = Tokenizer::tokenize("rust-2024 version-1.0");
    assert!(tokens.len() >= 2);
    // Hyphens and underscores are preserved, but periods are not
    assert!(tokens.iter().any(|t| t.text.contains("rust-2024")));
    // The period will separate "version-1" and "0"
    assert!(tokens.iter().any(|t| t.text.contains("version-1")));
}

#[test]
fn test_tokenize_long_text() {
    let text = "The quick brown fox jumps over the lazy dog. \
                This is a longer sentence with more words to test tokenization.";
    let tokens = Tokenizer::tokenize(text);
    
    // Should tokenize all words
    assert!(tokens.len() > 15);
    
    // Check positions are sequential
    for i in 0..tokens.len() {
        assert_eq!(tokens[i].position, i);
    }
}

#[test]
fn test_tokenize_repeated_words() {
    let tokens = Tokenizer::tokenize("test test test");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].text, "test");
    assert_eq!(tokens[1].text, "test");
    assert_eq!(tokens[2].text, "test");
    assert_eq!(tokens[0].position, 0);
    assert_eq!(tokens[1].position, 1);
    assert_eq!(tokens[2].position, 2);
}

#[test]
fn test_token_equality() {
    let token1 = Token {
        text: "test".to_string(),
        position: 0,
    };
    let token2 = Token {
        text: "test".to_string(),
        position: 0,
    };
    
    assert_eq!(token1, token2);
}

#[test]
fn test_token_clone() {
    let token = Token {
        text: "test".to_string(),
        position: 5,
    };
    let cloned = token.clone();
    
    assert_eq!(token.text, cloned.text);
    assert_eq!(token.position, cloned.position);
}

#[test]
fn test_tokenize_mixed_content() {
    let tokens = Tokenizer::tokenize("Hello123 world456 test-case_name");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].text, "hello123");
    assert_eq!(tokens[1].text, "world456");
    assert_eq!(tokens[2].text, "test-case_name");
}

#[test]
fn test_trigrams_overlap() {
    let trigrams = Tokenizer::trigrams("abcde");
    // abcde should produce: abc, bcd, cde
    assert_eq!(trigrams.len(), 3);
    assert_eq!(trigrams[0], "abc");
    assert_eq!(trigrams[1], "bcd");
    assert_eq!(trigrams[2], "cde");
}

//! Naming utilities for code generation
//!
//! Provides case conversion and pluralization helpers to reduce duplication
//! across generators.

/// Convert a string to PascalCase
pub fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                }
            }
        })
        .collect()
}

/// Convert a string to camelCase
pub fn to_camel_case(s: &str) -> String {
    let pascal = to_pascal_case(s);
    let mut chars = pascal.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
    }
}

/// Convert a string to kebab-case
pub fn to_kebab_case(s: &str) -> String {
    s.to_lowercase().replace('_', "-")
}

/// Convert a technical name to a human-readable format
pub fn humanize(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Simple pluralization heuristic
/// This is a basic implementation - we can replace it with an inflector crate later
pub fn pluralize(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    
    let lower = s.to_lowercase();
    
    // Handle common irregular plurals
    match lower.as_str() {
        "person" => return "people".to_string(),
        "child" => return "children".to_string(),
        "man" => return "men".to_string(),
        "woman" => return "women".to_string(),
        _ => {}
    }
    
    // Handle words ending in 'y' preceded by a consonant
    if lower.ends_with('y') && lower.len() > 1 {
        let before_y = lower.chars().nth(lower.len() - 2).unwrap();
        if !matches!(before_y, 'a' | 'e' | 'i' | 'o' | 'u') {
            return format!("{}ies", &s[..s.len() - 1]);
        }
    }
    
    // Handle words ending in 's', 'ss', 'sh', 'ch', 'x', 'z'
    if lower.ends_with("ss") || lower.ends_with("sh") || lower.ends_with("ch") 
        || lower.ends_with('x') || lower.ends_with('z') {
        return format!("{}es", s);
    }
    
    // Handle words ending in 's'
    if lower.ends_with('s') {
        return format!("{}es", s);
    }
    
    // Default: just add 's'
    format!("{}s", s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("user"), "User");
        assert_eq!(to_pascal_case("user_profile"), "UserProfile");
        assert_eq!(to_pascal_case("api_key"), "ApiKey");
    }

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("user"), "user");
        assert_eq!(to_camel_case("user_profile"), "userProfile");
        assert_eq!(to_camel_case("api_key"), "apiKey");
    }

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("user"), "user");
        assert_eq!(to_kebab_case("user_profile"), "user-profile");
        assert_eq!(to_kebab_case("api_key"), "api-key");
    }

    #[test]
    fn test_humanize() {
        assert_eq!(humanize("user"), "User");
        assert_eq!(humanize("user_profile"), "User Profile");
        assert_eq!(humanize("api_key"), "Api Key");
    }

    #[test]
    fn test_pluralize() {
        assert_eq!(pluralize("user"), "users");
        assert_eq!(pluralize("post"), "posts");
        assert_eq!(pluralize("class"), "classes");
        assert_eq!(pluralize("box"), "boxes");
        assert_eq!(pluralize("buzz"), "buzzes");
        assert_eq!(pluralize("category"), "categories");
        assert_eq!(pluralize("day"), "days");
        assert_eq!(pluralize("person"), "people");
    }
}

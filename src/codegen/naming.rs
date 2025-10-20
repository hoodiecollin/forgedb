//! Naming utilities for code generation
//!
//! Provides case conversion and pluralization helpers to reduce duplication
//! across generators. Uses heck for case conversion and inflector for pluralization.

use heck::{ToKebabCase, ToLowerCamelCase, ToPascalCase, ToSnakeCase};

/// Convert a string to PascalCase
pub fn to_pascal_case(s: &str) -> String {
    // Use heck's ToPascalCase
    ToPascalCase::to_pascal_case(s)
}

/// Convert a string to camelCase
pub fn to_camel_case(s: &str) -> String {
    // Use heck's ToLowerCamelCase
    ToLowerCamelCase::to_lower_camel_case(s)
}

/// Convert a string to kebab-case
pub fn to_kebab_case(s: &str) -> String {
    // Use heck's ToKebabCase
    ToKebabCase::to_kebab_case(s)
}

/// Convert a string to snake_case
pub fn to_snake_case(s: &str) -> String {
    // Use heck's ToSnakeCase
    ToSnakeCase::to_snake_case(s)
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

/// Pluralize a string using the inflector crate with custom handling for irregular words
pub fn pluralize(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    
    // Handle common irregular plurals first
    let lower = s.to_lowercase();
    let custom_plural = match lower.as_str() {
        "person" => Some("people"),
        "child" => Some("children"),
        "man" => Some("men"),
        "woman" => Some("women"),
        "tooth" => Some("teeth"),
        "foot" => Some("feet"),
        "mouse" => Some("mice"),
        "goose" => Some("geese"),
        _ => None,
    };
    
    if let Some(plural) = custom_plural {
        // Preserve the original casing
        if s.chars().next().unwrap().is_uppercase() {
            let mut result = plural.to_string();
            result.replace_range(0..1, &plural[0..1].to_uppercase());
            result
        } else {
            plural.to_string()
        }
    } else {
        // Use inflector for everything else
        inflector::string::pluralize::to_plural(s)
    }
}

/// Singularize a string using the inflector crate with custom handling for irregular words
pub fn singularize(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    
    // Handle common irregular singulars first
    let lower = s.to_lowercase();
    let custom_singular = match lower.as_str() {
        "people" => Some("person"),
        "children" => Some("child"),
        "men" => Some("man"),
        "women" => Some("woman"),
        "teeth" => Some("tooth"),
        "feet" => Some("foot"),
        "mice" => Some("mouse"),
        "geese" => Some("goose"),
        _ => None,
    };
    
    if let Some(singular) = custom_singular {
        // Preserve the original casing
        if s.chars().next().unwrap().is_uppercase() {
            let mut result = singular.to_string();
            result.replace_range(0..1, &singular[0..1].to_uppercase());
            result
        } else {
            singular.to_string()
        }
    } else {
        // Use inflector for everything else
        inflector::string::singularize::to_singular(s)
    }
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
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("User"), "user");
        assert_eq!(to_snake_case("UserProfile"), "user_profile");
        assert_eq!(to_snake_case("APIKey"), "api_key");
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
        assert_eq!(pluralize("category"), "categories");
        assert_eq!(pluralize("day"), "days");
        assert_eq!(pluralize("person"), "people");
    }

    #[test]
    fn test_singularize() {
        assert_eq!(singularize("users"), "user");
        assert_eq!(singularize("posts"), "post");
        assert_eq!(singularize("classes"), "class");
        assert_eq!(singularize("categories"), "category");
        assert_eq!(singularize("people"), "person");
    }
}

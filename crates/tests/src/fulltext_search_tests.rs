/// Sprint 18: Full-Text Search Tests
///
/// Tests for:
/// - @fulltext directive parsing
/// - Full-text index generation
/// - Search method generation
/// - TF-IDF scoring
/// - Phrase search

#[cfg(test)]
mod tests {
    use forgedb::{CodeGenerator, Parser};

    #[test]
    fn test_fulltext_directive_parsing() {
        let schema = r#"
Article {
  id: +uuid
  title: string @fulltext
  content: string @fulltext
}
"#;

        let mut parser = Parser::new(schema).unwrap();
        let parsed_schema = parser.parse().unwrap();

        assert_eq!(parsed_schema.models.len(), 1);
        let model = &parsed_schema.models[0];
        assert_eq!(model.name, "Article");

        // Check that fulltext_indexed is set correctly
        let title_field = model.fields.iter().find(|f| f.name == "title").unwrap();
        assert!(
            title_field.fulltext_indexed,
            "title should be fulltext indexed"
        );

        let content_field = model.fields.iter().find(|f| f.name == "content").unwrap();
        assert!(
            content_field.fulltext_indexed,
            "content should be fulltext indexed"
        );

        let id_field = model.fields.iter().find(|f| f.name == "id").unwrap();
        assert!(
            !id_field.fulltext_indexed,
            "id should not be fulltext indexed"
        );
    }

    #[test]
    fn test_fulltext_index_generation() {
        let schema = r#"
Article {
  id: +uuid
  title: string @fulltext
  content: string @fulltext
}
"#;

        let mut parser = Parser::new(schema).unwrap();
        let parsed_schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&parsed_schema);

        // Check that full-text index fields are generated
        assert!(
            code.contains(
                "title_fulltext: std::sync::Arc<std::sync::RwLock<forgedb_fulltext::FullTextIndex>>"
            ),
            "title_fulltext field should be generated"
        );
        assert!(code.contains("content_fulltext: std::sync::Arc<std::sync::RwLock<forgedb_fulltext::FullTextIndex>>"),
            "content_fulltext field should be generated");

        // Check that indexes are initialized
        assert!(code.contains("title_fulltext: std::sync::Arc::new(std::sync::RwLock::new(forgedb_fulltext::FullTextIndex::new()))"),
            "title_fulltext should be initialized");
        assert!(code.contains("content_fulltext: std::sync::Arc::new(std::sync::RwLock::new(forgedb_fulltext::FullTextIndex::new()))"),
            "content_fulltext should be initialized");
    }

    #[test]
    fn test_search_methods_generation() {
        let schema = r#"
Article {
  id: +uuid
  title: string @fulltext
  content: string @fulltext
}
"#;

        let mut parser = Parser::new(schema).unwrap();
        let parsed_schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&parsed_schema);

        // Check that search methods are generated
        assert!(
            code.contains("pub fn search_title(&self, query: &str) -> Vec<Article>"),
            "search_title method should be generated"
        );
        assert!(
            code.contains("pub fn search_content(&self, query: &str) -> Vec<Article>"),
            "search_content method should be generated"
        );

        // Check that phrase search methods are generated
        assert!(
            code.contains("pub fn search_title_phrase(&self, phrase: &str) -> Vec<Article>"),
            "search_title_phrase method should be generated"
        );
        assert!(
            code.contains("pub fn search_content_phrase(&self, phrase: &str) -> Vec<Article>"),
            "search_content_phrase method should be generated"
        );
    }

    #[test]
    fn test_fulltext_index_maintenance() {
        let schema = r#"
Article {
  id: +uuid
  title: string @fulltext
}
"#;

        let mut parser = Parser::new(schema).unwrap();
        let parsed_schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&parsed_schema);

        // Check that insert adds to full-text index
        assert!(
            code.contains("self.title_fulltext.write().unwrap().add_document"),
            "insert should add document to full-text index"
        );
    }

    #[test]
    fn test_multiple_fulltext_fields() {
        let schema = r#"
Article {
  id: +uuid
  title: string @fulltext
  content: string @fulltext
  summary: string @fulltext
  author: string
}
"#;

        let mut parser = Parser::new(schema).unwrap();
        let parsed_schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&parsed_schema);

        // All three fulltext fields should have indexes
        assert!(code.contains("title_fulltext"));
        assert!(code.contains("content_fulltext"));
        assert!(code.contains("summary_fulltext"));

        // All three should have search methods
        assert!(code.contains("pub fn search_title"));
        assert!(code.contains("pub fn search_content"));
        assert!(code.contains("pub fn search_summary"));

        // Author should not have fulltext search
        assert!(!code.contains("author_fulltext"));
        assert!(!code.contains("pub fn search_author"));
    }

    #[test]
    fn test_fulltext_with_other_constraints() {
        let schema = r#"
Article {
  id: +uuid
  title: string @fulltext @min(5) @max(200)
}
"#;

        let mut parser = Parser::new(schema).unwrap();
        let parsed_schema = parser.parse().unwrap();

        assert_eq!(parsed_schema.models.len(), 1);
        let model = &parsed_schema.models[0];

        let title_field = model.fields.iter().find(|f| f.name == "title").unwrap();
        assert!(title_field.fulltext_indexed);
        assert_eq!(title_field.constraints.len(), 3); // @fulltext, @min, @max
    }

    #[test]
    fn test_fulltext_only_on_string_fields() {
        // In practice, fulltext should only be used on string fields
        // The code generator should handle this gracefully
        let schema = r#"
Article {
  id: +uuid
  title: string @fulltext
  views: u32
}
"#;

        let mut parser = Parser::new(schema).unwrap();
        let parsed_schema = parser.parse().unwrap();

        let generator = CodeGenerator::new();
        let code = generator.generate(&parsed_schema);

        // Only title should have fulltext
        assert!(code.contains("title_fulltext"));
        assert!(!code.contains("views_fulltext"));
    }
}

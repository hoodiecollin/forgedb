use crate::ast::{RelationPair, Schema};

pub struct RelationTraversalGenerator;

impl RelationTraversalGenerator {
    pub fn new() -> Self {
        RelationTraversalGenerator
    }

    pub fn generate_relation_traversal_method(
        &self,
        code: &mut String,
        relation: &RelationPair,
        schema: &Schema,
    ) {
        // Generate parent.children() method
        // e.g., user.posts() -> Vec<Post>
        let parent_storage = relation.parent_model.to_lowercase();
        let child_storage = relation.child_model.to_lowercase();
        let method_name = format!("{}_{}", parent_storage, relation.parent_field); // e.g., user_posts

        let parent_model = schema.find_model(&relation.parent_model).unwrap();
        let id_field = parent_model
            .fields
            .iter()
            .find(|f| f.auto_generate)
            .unwrap();

        code.push_str(&format!(
            "    pub fn {}(&self, {}_id: {}) -> Vec<{}> {{\n",
            method_name,
            parent_storage,
            id_field.field_type.to_rust_type(),
            relation.child_model
        ));

        code.push_str(&format!(
            "        self.{}.find_by_{}_id({}_id)\n",
            child_storage, parent_storage, parent_storage
        ));

        code.push_str("    }\n\n");
    }

    pub fn generate_reverse_lookup_method(
        &self,
        code: &mut String,
        relation: &RelationPair,
        schema: &Schema,
    ) {
        // Generate child.parent() method
        // e.g., post.author() -> Option<User>
        let parent_storage = relation.parent_model.to_lowercase();
        let child_storage = relation.child_model.to_lowercase();
        let method_name = format!("{}_{}", child_storage, relation.child_field); // e.g., post_author

        let child_model = schema.find_model(&relation.child_model).unwrap();
        let id_field = child_model.fields.iter().find(|f| f.auto_generate).unwrap();

        let return_type = if relation.is_required {
            format!("Option<{}>", relation.parent_model)
        } else {
            format!("Option<{}>", relation.parent_model)
        };

        code.push_str(&format!(
            "    pub fn {}(&self, {}_id: {}) -> {} {{\n",
            method_name,
            child_storage,
            id_field.field_type.to_rust_type(),
            return_type
        ));

        code.push_str(&format!(
            "        if let Some(child) = self.{}.get({}_id) {{\n",
            child_storage, child_storage
        ));

        code.push_str(&format!(
            "            return self.{}.get(child.{}_id);\n",
            parent_storage, relation.child_field
        ));

        code.push_str("        }\n");
        code.push_str("        None\n");
        code.push_str("    }\n\n");
    }
}

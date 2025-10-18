use crate::ast::{ManyToManyRelation, Schema};

pub struct JunctionTableGenerator;

impl JunctionTableGenerator {
    pub fn new() -> Self {
        JunctionTableGenerator
    }

    /// Generate junction table name from two models
    pub fn junction_table_name(m2m: &ManyToManyRelation) -> String {
        // Sort model names alphabetically for consistency
        let (model1, model2, _field1, _field2) = if m2m.model1 < m2m.model2 {
            (&m2m.model1, &m2m.model2, &m2m.field1, &m2m.field2)
        } else {
            (&m2m.model2, &m2m.model1, &m2m.field2, &m2m.field1)
        };

        // If there's only one M:N between these models, use simple name
        // Otherwise include field name to differentiate
        format!("{}{}Junction", model1, model2)
    }

    /// Generate junction table storage struct
    pub fn generate_junction_table(&self, m2m: &ManyToManyRelation, schema: &Schema) -> String {
        let mut code = String::new();
        let junction_name = Self::junction_table_name(m2m);

        // Find ID types for both models
        let model1 = schema.find_model(&m2m.model1).unwrap();
        let model2 = schema.find_model(&m2m.model2).unwrap();
        let id1_field = model1.fields.iter().find(|f| f.auto_generate).unwrap();
        let id2_field = model2.fields.iter().find(|f| f.auto_generate).unwrap();
        let id1_type = id1_field.field_type.to_rust_type();
        let id2_type = id2_field.field_type.to_rust_type();

        // Junction record struct
        code.push_str(&format!("#[derive(Debug, Clone, PartialEq)]\n"));
        code.push_str(&format!("pub struct {} {{\n", junction_name));
        code.push_str(&format!(
            "    pub {}_id: {},\n",
            m2m.model1.to_lowercase(),
            id1_type
        ));
        code.push_str(&format!(
            "    pub {}_id: {},\n",
            m2m.model2.to_lowercase(),
            id2_type
        ));
        code.push_str("}\n\n");

        // Junction storage struct
        code.push_str(&format!("pub struct {}Storage {{\n", junction_name));
        code.push_str(&format!("    records: Vec<{}>,\n", junction_name));
        code.push_str(&format!(
            "    {}_to_{}_index: std::collections::HashMap<{}, Vec<{}>>,\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            id1_type,
            id2_type
        ));
        code.push_str(&format!(
            "    {}_to_{}_index: std::collections::HashMap<{}, Vec<{}>>,\n",
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase(),
            id2_type,
            id1_type
        ));
        code.push_str("}\n\n");

        // Implementation
        code.push_str(&format!("impl {}Storage {{\n", junction_name));

        // new()
        code.push_str("    pub fn new() -> Self {\n");
        code.push_str(&format!("        {}Storage {{\n", junction_name));
        code.push_str("            records: Vec::new(),\n");
        code.push_str(&format!(
            "            {}_to_{}_index: std::collections::HashMap::new(),\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str(&format!(
            "            {}_to_{}_index: std::collections::HashMap::new(),\n",
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase()
        ));
        code.push_str("        }\n");
        code.push_str("    }\n\n");

        // add_relation()
        code.push_str(&format!(
            "    pub fn add_relation(&mut self, {}_id: {}, {}_id: {}) {{\n",
            m2m.model1.to_lowercase(),
            id1_type,
            m2m.model2.to_lowercase(),
            id2_type
        ));
        code.push_str(&format!("        // Check if relation already exists\n"));
        code.push_str(&format!(
            "        if self.has_relation({}_id, {}_id) {{\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str("            return;\n");
        code.push_str("        }\n\n");
        code.push_str(&format!("        let record = {} {{\n", junction_name));
        code.push_str(&format!("            {}_id,\n", m2m.model1.to_lowercase()));
        code.push_str(&format!("            {}_id,\n", m2m.model2.to_lowercase()));
        code.push_str("        };\n");
        code.push_str("        self.records.push(record);\n");
        code.push_str(&format!(
            "        self.{}_to_{}_index.entry({}_id).or_insert_with(Vec::new).push({}_id);\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str(&format!(
            "        self.{}_to_{}_index.entry({}_id).or_insert_with(Vec::new).push({}_id);\n",
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase()
        ));
        code.push_str("    }\n\n");

        // remove_relation()
        code.push_str(&format!(
            "    pub fn remove_relation(&mut self, {}_id: {}, {}_id: {}) {{\n",
            m2m.model1.to_lowercase(),
            id1_type,
            m2m.model2.to_lowercase(),
            id2_type
        ));
        code.push_str(&format!(
            "        self.records.retain(|r| !(r.{}_id == {}_id && r.{}_id == {}_id));\n",
            m2m.model1.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str(&format!(
            "        if let Some(ids) = self.{}_to_{}_index.get_mut(&{}_id) {{\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase()
        ));
        code.push_str(&format!(
            "            ids.retain(|&id| id != {}_id);\n",
            m2m.model2.to_lowercase()
        ));
        code.push_str("        }\n");
        code.push_str(&format!(
            "        if let Some(ids) = self.{}_to_{}_index.get_mut(&{}_id) {{\n",
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str(&format!(
            "            ids.retain(|&id| id != {}_id);\n",
            m2m.model1.to_lowercase()
        ));
        code.push_str("        }\n");
        code.push_str("    }\n\n");

        // get_related_ids() for model1 -> model2
        code.push_str(&format!(
            "    pub fn get_{}_{}(&self, {}_id: {}) -> Vec<{}> {{\n",
            m2m.model1.to_lowercase(),
            m2m.field1,
            m2m.model1.to_lowercase(),
            id1_type,
            id2_type
        ));
        code.push_str(&format!(
            "        self.{}_to_{}_index.get(&{}_id)\n",
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase()
        ));
        code.push_str("            .map(|ids| ids.clone())\n");
        code.push_str("            .unwrap_or_else(Vec::new)\n");
        code.push_str("    }\n\n");

        // get_related_ids() for model2 -> model1
        code.push_str(&format!(
            "    pub fn get_{}_{}(&self, {}_id: {}) -> Vec<{}> {{\n",
            m2m.model2.to_lowercase(),
            m2m.field2,
            m2m.model2.to_lowercase(),
            id2_type,
            id1_type
        ));
        code.push_str(&format!(
            "        self.{}_to_{}_index.get(&{}_id)\n",
            m2m.model2.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str("            .map(|ids| ids.clone())\n");
        code.push_str("            .unwrap_or_else(Vec::new)\n");
        code.push_str("    }\n\n");

        // has_relation()
        code.push_str(&format!(
            "    pub fn has_relation(&self, {}_id: {}, {}_id: {}) -> bool {{\n",
            m2m.model1.to_lowercase(),
            id1_type,
            m2m.model2.to_lowercase(),
            id2_type
        ));
        code.push_str(&format!(
            "        self.records.iter().any(|r| r.{}_id == {}_id && r.{}_id == {}_id)\n",
            m2m.model1.to_lowercase(),
            m2m.model1.to_lowercase(),
            m2m.model2.to_lowercase(),
            m2m.model2.to_lowercase()
        ));
        code.push_str("    }\n");

        code.push_str("}\n\n");

        code
    }
}

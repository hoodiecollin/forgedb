use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::Asc
    }
}

impl SortOrder {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "asc" | "ascending" => Some(SortOrder::Asc),
            "desc" | "descending" => Some(SortOrder::Desc),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sort {
    pub field: String,
    pub order: SortOrder,
}

impl Sort {
    pub fn new(field: impl Into<String>, order: SortOrder) -> Self {
        Self {
            field: field.into(),
            order,
        }
    }

    pub fn from_params(sort_field: Option<String>, order_str: Option<String>) -> Option<Self> {
        sort_field.map(|field| {
            let order = order_str
                .and_then(|s| SortOrder::from_str(&s))
                .unwrap_or_default();
            Sort { field, order }
        })
    }

    pub fn is_ascending(&self) -> bool {
        matches!(self.order, SortOrder::Asc)
    }

    pub fn is_descending(&self) -> bool {
        matches!(self.order, SortOrder::Desc)
    }
}
